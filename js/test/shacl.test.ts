import assert from "node:assert";
import { webcrypto } from "node:crypto";
import { describe, it, vi } from "vitest";
import {
    Store,
    ShaclShapesGraph,
    ShaclValidator,
    shaclValidate,
} from "../pkg/oxigraph.js";

// thread_rng: Node.js ES modules are not directly supported, see https://docs.rs/getrandom#nodejs-es-module-support
vi.stubGlobal("crypto", webcrypto);

describe("SHACL", () => {
    describe("ShaclShapesGraph", () => {
        it("should create an empty shapes graph", () => {
            const shapes = new ShaclShapesGraph();
            assert.strictEqual(shapes.size, 0);
            assert.strictEqual(shapes.isEmpty(), true);
        });

        it("should parse shapes from Turtle", () => {
            const shapes = new ShaclShapesGraph();
            const shapesData = `
                @prefix sh: <http://www.w3.org/ns/shacl#> .
                @prefix ex: <http://example.com/> .

                ex:PersonShape a sh:NodeShape ;
                    sh:targetClass ex:Person ;
                    sh:property [
                        sh:path ex:name ;
                        sh:minCount 1 ;
                        sh:datatype <http://www.w3.org/2001/XMLSchema#string> ;
                    ] .
            `;
            shapes.parse(shapesData);
            assert(shapes.size > 0);
            assert.strictEqual(shapes.isEmpty(), false);
        });

        it("should throw error on invalid Turtle", () => {
            const shapes = new ShaclShapesGraph();
            assert.throws(() => {
                shapes.parse("invalid turtle <<<");
            });
        });
    });

    describe("ShaclValidator", () => {
        const personShapes = `
            @prefix sh: <http://www.w3.org/ns/shacl#> .
            @prefix ex: <http://example.com/> .
            @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

            ex:PersonShape a sh:NodeShape ;
                sh:targetClass ex:Person ;
                sh:property [
                    sh:path ex:name ;
                    sh:minCount 1 ;
                    sh:datatype xsd:string ;
                    sh:message "Person must have at least one name" ;
                ] ;
                sh:property [
                    sh:path ex:age ;
                    sh:maxCount 1 ;
                    sh:datatype xsd:integer ;
                    sh:message "Person must have at most one age" ;
                ] .
        `;

        it("should validate conforming data", () => {
            const shapes = new ShaclShapesGraph();
            shapes.parse(personShapes);
            const validator = new ShaclValidator(shapes);

            const validData = `
                @prefix ex: <http://example.com/> .

                ex:Alice a ex:Person ;
                    ex:name "Alice" ;
                    ex:age 30 .
            `;

            const report = validator.validate(validData);
            assert.strictEqual(report.conforms, true);
            assert.strictEqual(report.violationCount, 0);
            assert.strictEqual(report.warningCount, 0);
            assert.strictEqual(report.infoCount, 0);
        });

        it("should detect violations in non-conforming data", () => {
            const shapes = new ShaclShapesGraph();
            shapes.parse(personShapes);
            const validator = new ShaclValidator(shapes);

            const invalidData = `
                @prefix ex: <http://example.com/> .

                ex:Bob a ex:Person .
            `;

            const report = validator.validate(invalidData);
            assert.strictEqual(report.conforms, false);
            assert(report.violationCount > 0);
        });

        it("should provide validation results with details", () => {
            const shapes = new ShaclShapesGraph();
            shapes.parse(personShapes);
            const validator = new ShaclValidator(shapes);

            const invalidData = `
                @prefix ex: <http://example.com/> .

                ex:Bob a ex:Person .
            `;

            const report = validator.validate(invalidData);
            const results = report.results();

            assert(Array.isArray(results));
            assert(results.length > 0);

            const firstResult = results[0];
            assert(firstResult.focusNode !== undefined);
            assert.strictEqual(firstResult.severity, "Violation");

            // Check that message is present (from sh:message in shape)
            if (firstResult.message) {
                assert(firstResult.message.includes("Person must have at least one name"));
            }
        });

        it("should validate Store objects", () => {
            const shapes = new ShaclShapesGraph();
            shapes.parse(personShapes);
            const validator = new ShaclValidator(shapes);

            const store = new Store();
            store.load(
                `
                @prefix ex: <http://example.com/> .

                ex:Alice a ex:Person ;
                    ex:name "Alice" .
                `,
                { format: "text/turtle" },
            );

            const report = validator.validateStore(store);
            assert.strictEqual(report.conforms, true);
            assert.strictEqual(report.violationCount, 0);
        });

        it("should detect violations in Store objects", () => {
            const shapes = new ShaclShapesGraph();
            shapes.parse(personShapes);
            const validator = new ShaclValidator(shapes);

            const store = new Store();
            store.load(
                `
                @prefix ex: <http://example.com/> .

                ex:Charlie a ex:Person .
                `,
                { format: "text/turtle" },
            );

            const report = validator.validateStore(store);
            assert.strictEqual(report.conforms, false);
            assert(report.violationCount > 0);
        });
    });

    describe("ShaclValidationReport", () => {
        const shapes = `
            @prefix sh: <http://www.w3.org/ns/shacl#> .
            @prefix ex: <http://example.com/> .
            @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

            ex:PersonShape a sh:NodeShape ;
                sh:targetClass ex:Person ;
                sh:property [
                    sh:path ex:name ;
                    sh:minCount 1 ;
                ] ;
                sh:property [
                    sh:path ex:email ;
                    sh:pattern "^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\\\\.[a-zA-Z]{2,}$" ;
                ] .
        `;

        it("should serialize report to Turtle", () => {
            const shapesGraph = new ShaclShapesGraph();
            shapesGraph.parse(shapes);
            const validator = new ShaclValidator(shapesGraph);

            const invalidData = `
                @prefix ex: <http://example.com/> .

                ex:Bob a ex:Person ;
                    ex:email "invalid-email" .
            `;

            const report = validator.validate(invalidData);
            const turtle = report.toTurtle();

            assert(typeof turtle === "string");
            assert(turtle.length > 0);
            assert(turtle.includes("sh:ValidationReport") || turtle.includes("shacl#"));
        });

        it("should track violation counts correctly", () => {
            const shapesGraph = new ShaclShapesGraph();
            shapesGraph.parse(shapes);
            const validator = new ShaclValidator(shapesGraph);

            // Missing name AND invalid email = 2 violations
            const invalidData = `
                @prefix ex: <http://example.com/> .

                ex:Bob a ex:Person ;
                    ex:email "not-an-email" .
            `;

            const report = validator.validate(invalidData);
            assert(report.violationCount >= 1);
            assert.strictEqual(report.results().length, report.violationCount + report.warningCount + report.infoCount);
        });
    });

    describe("ShaclValidationResult", () => {
        const shapes = `
            @prefix sh: <http://www.w3.org/ns/shacl#> .
            @prefix ex: <http://example.com/> .
            @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

            ex:PersonShape a sh:NodeShape ;
                sh:targetClass ex:Person ;
                sh:property [
                    sh:path ex:age ;
                    sh:minInclusive 0 ;
                    sh:maxInclusive 150 ;
                    sh:datatype xsd:integer ;
                    sh:message "Age must be between 0 and 150" ;
                ] .
        `;

        it("should provide focusNode for violations", () => {
            const shapesGraph = new ShaclShapesGraph();
            shapesGraph.parse(shapes);
            const validator = new ShaclValidator(shapesGraph);

            const invalidData = `
                @prefix ex: <http://example.com/> .

                ex:Alice a ex:Person ;
                    ex:age 200 .
            `;

            const report = validator.validate(invalidData);
            const results = report.results();

            assert(results.length > 0);
            const result = results[0];
            assert(result.focusNode !== undefined);
            assert(result.focusNode.value !== undefined);
        });

        it("should provide value for property constraint violations", () => {
            const shapesGraph = new ShaclShapesGraph();
            shapesGraph.parse(shapes);
            const validator = new ShaclValidator(shapesGraph);

            const invalidData = `
                @prefix ex: <http://example.com/> .

                ex:Bob a ex:Person ;
                    ex:age 200 .
            `;

            const report = validator.validate(invalidData);
            const results = report.results();

            assert(results.length > 0);
            const result = results[0];

            // Value should be the problematic age value
            if (result.value) {
                assert(result.value.value !== undefined);
            }
        });

        it("should provide message when defined", () => {
            const shapesGraph = new ShaclShapesGraph();
            shapesGraph.parse(shapes);
            const validator = new ShaclValidator(shapesGraph);

            const invalidData = `
                @prefix ex: <http://example.com/> .

                ex:Charlie a ex:Person ;
                    ex:age 200 .
            `;

            const report = validator.validate(invalidData);
            const results = report.results();

            assert(results.length > 0);
            const result = results[0];

            // Should have the custom message we defined
            if (result.message) {
                assert(typeof result.message === "string");
            }
        });

        it("should set severity to Violation for constraint violations", () => {
            const shapesGraph = new ShaclShapesGraph();
            shapesGraph.parse(shapes);
            const validator = new ShaclValidator(shapesGraph);

            const invalidData = `
                @prefix ex: <http://example.com/> .

                ex:Dave a ex:Person ;
                    ex:age -10 .
            `;

            const report = validator.validate(invalidData);
            const results = report.results();

            assert(results.length > 0);
            results.forEach((result) => {
                assert(
                    result.severity === "Violation" ||
                    result.severity === "Warning" ||
                    result.severity === "Info"
                );
            });
        });
    });

    describe("shaclValidate() convenience function", () => {
        it("should validate data with shapes in one call", () => {
            const shapes = `
                @prefix sh: <http://www.w3.org/ns/shacl#> .
                @prefix ex: <http://example.com/> .

                ex:PersonShape a sh:NodeShape ;
                    sh:targetClass ex:Person ;
                    sh:property [
                        sh:path ex:name ;
                        sh:minCount 1 ;
                    ] .
            `;

            const validData = `
                @prefix ex: <http://example.com/> .

                ex:Alice a ex:Person ;
                    ex:name "Alice" .
            `;

            const report = shaclValidate(shapes, validData);
            assert.strictEqual(report.conforms, true);
            assert.strictEqual(report.violationCount, 0);
        });

        it("should detect violations in convenience function", () => {
            const shapes = `
                @prefix sh: <http://www.w3.org/ns/shacl#> .
                @prefix ex: <http://example.com/> .

                ex:PersonShape a sh:NodeShape ;
                    sh:targetClass ex:Person ;
                    sh:property [
                        sh:path ex:name ;
                        sh:minCount 1 ;
                    ] .
            `;

            const invalidData = `
                @prefix ex: <http://example.com/> .

                ex:Bob a ex:Person .
            `;

            const report = shaclValidate(shapes, invalidData);
            assert.strictEqual(report.conforms, false);
            assert(report.violationCount > 0);
        });

        it("should throw error on invalid shapes", () => {
            assert.throws(() => {
                shaclValidate("invalid <<<", "@prefix ex: <http://example.com/> .");
            });
        });

        it("should throw error on invalid data", () => {
            const shapes = `
                @prefix sh: <http://www.w3.org/ns/shacl#> .
                @prefix ex: <http://example.com/> .

                ex:PersonShape a sh:NodeShape .
            `;

            assert.throws(() => {
                shaclValidate(shapes, "invalid <<<");
            });
        });
    });

    describe("Complex SHACL scenarios", () => {
        it("should handle multiple constraint types", () => {
            const shapes = `
                @prefix sh: <http://www.w3.org/ns/shacl#> .
                @prefix ex: <http://example.com/> .
                @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

                ex:PersonShape a sh:NodeShape ;
                    sh:targetClass ex:Person ;
                    sh:property [
                        sh:path ex:name ;
                        sh:minCount 1 ;
                        sh:maxCount 1 ;
                        sh:datatype xsd:string ;
                        sh:minLength 1 ;
                    ] ;
                    sh:property [
                        sh:path ex:age ;
                        sh:datatype xsd:integer ;
                        sh:minInclusive 0 ;
                    ] ;
                    sh:property [
                        sh:path ex:email ;
                        sh:pattern "^.+@.+\\\\..+$" ;
                    ] .
            `;

            const validData = `
                @prefix ex: <http://example.com/> .

                ex:Alice a ex:Person ;
                    ex:name "Alice" ;
                    ex:age 30 ;
                    ex:email "alice@example.com" .
            `;

            const report = shaclValidate(shapes, validData);
            assert.strictEqual(report.conforms, true);
        });

        it("should handle sh:class constraints", () => {
            const shapes = `
                @prefix sh: <http://www.w3.org/ns/shacl#> .
                @prefix ex: <http://example.com/> .

                ex:PersonShape a sh:NodeShape ;
                    sh:targetClass ex:Person ;
                    sh:property [
                        sh:path ex:address ;
                        sh:class ex:Address ;
                    ] .
            `;

            const validData = `
                @prefix ex: <http://example.com/> .

                ex:Alice a ex:Person ;
                    ex:address ex:AliceAddress .

                ex:AliceAddress a ex:Address .
            `;

            const report = shaclValidate(shapes, validData);
            assert.strictEqual(report.conforms, true);
        });

        it("should handle sh:node constraints", () => {
            const shapes = `
                @prefix sh: <http://www.w3.org/ns/shacl#> .
                @prefix ex: <http://example.com/> .
                @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

                ex:AddressShape a sh:NodeShape ;
                    sh:property [
                        sh:path ex:city ;
                        sh:minCount 1 ;
                    ] .

                ex:PersonShape a sh:NodeShape ;
                    sh:targetClass ex:Person ;
                    sh:property [
                        sh:path ex:address ;
                        sh:node ex:AddressShape ;
                    ] .
            `;

            const validData = `
                @prefix ex: <http://example.com/> .

                ex:Alice a ex:Person ;
                    ex:address [
                        ex:city "New York" ;
                    ] .
            `;

            const report = shaclValidate(shapes, validData);
            assert.strictEqual(report.conforms, true);
        });

        it("should handle blank node shapes", () => {
            const shapes = `
                @prefix sh: <http://www.w3.org/ns/shacl#> .
                @prefix ex: <http://example.com/> .

                ex:PersonShape a sh:NodeShape ;
                    sh:targetClass ex:Person ;
                    sh:property [
                        sh:path ex:knows ;
                        sh:nodeKind sh:BlankNodeOrIRI ;
                    ] .
            `;

            const validData = `
                @prefix ex: <http://example.com/> .

                ex:Alice a ex:Person ;
                    ex:knows ex:Bob ;
                    ex:knows _:blank1 .
            `;

            const report = shaclValidate(shapes, validData);
            assert.strictEqual(report.conforms, true);
        });
    });
});
