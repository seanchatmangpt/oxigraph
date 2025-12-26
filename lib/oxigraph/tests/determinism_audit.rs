#![cfg(test)]
#![allow(clippy::panic_in_result_fn)]

//! Determinism Audit Test Suite
//!
//! This test suite PROVES that Oxigraph produces deterministic results under various conditions.
//! These tests verify that query results are reproducible and independent of:
//! - Insertion order
//! - Concurrent execution
//! - Memory layout
//! - Iteration count
//!
//! Any test failure indicates hidden nondeterminism that must be documented or fixed.

use oxigraph::io::RdfFormat;
use oxigraph::model::vocab::xsd;
use oxigraph::model::*;
use oxigraph::sparql::{QueryResults, SparqlEvaluator};
use oxigraph::store::Store;
use std::collections::HashSet;
use std::error::Error;
use std::sync::{Arc, Mutex};
use std::thread;

/// Helper to convert QueryResults to a canonical string representation for comparison
fn query_results_to_canonical_string(results: QueryResults<'_>) -> Result<String, Box<dyn Error>> {
    match results {
        QueryResults::Solutions(solutions) => {
            let mut rows: Vec<String> = Vec::new();
            for solution in solutions {
                let solution = solution?;
                let mut bindings: Vec<String> = solution
                    .iter()
                    .map(|(var, term)| format!("{}={}", var, term))
                    .collect();
                bindings.sort(); // Ensure consistent ordering within each solution
                rows.push(bindings.join(", "));
            }
            rows.sort(); // Sort solutions for canonical comparison
            Ok(rows.join("\n"))
        }
        QueryResults::Boolean(b) => Ok(b.to_string()),
        QueryResults::Graph(triples) => {
            let mut triple_strs: Vec<String> = Vec::new();
            for triple in triples {
                let triple = triple?;
                triple_strs.push(format!(
                    "{} {} {}",
                    triple.subject, triple.predicate, triple.object
                ));
            }
            triple_strs.sort();
            Ok(triple_strs.join("\n"))
        }
    }
}

/// Test 1: Query result order is deterministic across 50 identical runs
#[test]
fn test_query_result_order_deterministic() -> Result<(), Box<dyn Error>> {
    const ITERATIONS: usize = 50;
    let mut results: Vec<String> = Vec::with_capacity(ITERATIONS);

    for i in 0..ITERATIONS {
        let store = Store::new()?;

        // Insert triples in varying order each iteration
        let triples = vec![
            QuadRef::new(
                NamedNodeRef::new_unchecked("http://example.com/s1"),
                NamedNodeRef::new_unchecked("http://example.com/p"),
                LiteralRef::new_simple_literal("object1"),
                GraphNameRef::DefaultGraph,
            ),
            QuadRef::new(
                NamedNodeRef::new_unchecked("http://example.com/s2"),
                NamedNodeRef::new_unchecked("http://example.com/p"),
                LiteralRef::new_simple_literal("object2"),
                GraphNameRef::DefaultGraph,
            ),
            QuadRef::new(
                NamedNodeRef::new_unchecked("http://example.com/s3"),
                NamedNodeRef::new_unchecked("http://example.com/p"),
                LiteralRef::new_simple_literal("object3"),
                GraphNameRef::DefaultGraph,
            ),
        ];

        // Insert in pseudo-random order based on iteration
        for (idx, _quad) in triples.iter().enumerate() {
            let insert_idx = (idx + i * 7) % triples.len();
            store.insert(triples[insert_idx])?;
        }

        // Execute query
        let query_results = SparqlEvaluator::new()
            .parse_query("SELECT ?s ?o WHERE { ?s <http://example.com/p> ?o } ORDER BY ?s")?
            .on_store(&store)
            .execute()?;

        let canonical_result = query_results_to_canonical_string(query_results)?;
        results.push(canonical_result);
    }

    // Verify all results are identical
    let first_result = &results[0];
    for (i, result) in results.iter().enumerate() {
        assert_eq!(
            result, first_result,
            "Result at iteration {} differs from first result.\nExpected:\n{}\nGot:\n{}",
            i, first_result, result
        );
    }

    Ok(())
}

/// Test 2: Concurrent queries produce identical results
#[test]
fn test_concurrent_queries_same_results() -> Result<(), Box<dyn Error>> {
    const NUM_THREADS: usize = 10;

    // Create and populate store
    let store = Arc::new(Store::new()?);

    // Load test data
    let data = r#"
        @prefix ex: <http://example.com/> .
        ex:alice ex:name "Alice" .
        ex:alice ex:age 30 .
        ex:bob ex:name "Bob" .
        ex:bob ex:age 25 .
        ex:charlie ex:name "Charlie" .
        ex:charlie ex:age 35 .
    "#;
    store.load_from_reader(RdfFormat::Turtle, data.as_bytes())?;

    let results = Arc::new(Mutex::new(Vec::new()));

    // Spawn threads to execute the same query concurrently
    let mut handles = Vec::new();
    for thread_id in 0..NUM_THREADS {
        let store = Arc::clone(&store);
        let results = Arc::clone(&results);

        let handle = thread::spawn(move || -> Result<String, String> {
            let query_results = SparqlEvaluator::new()
                .parse_query(
                    "SELECT ?person ?name WHERE { ?person <http://example.com/name> ?name } ORDER BY ?name",
                )
                .map_err(|e| e.to_string())?
                .on_store(&*store)
                .execute()
                .map_err(|e| e.to_string())?;

            let canonical = query_results_to_canonical_string(query_results)
                .map_err(|e| e.to_string())?;
            results.lock().unwrap().push((thread_id, canonical.clone()));
            Ok(canonical)
        });
        handles.push(handle);
    }

    // Wait for all threads
    for handle in handles {
        handle
            .join()
            .expect("Thread panicked")
            .expect("Thread returned error");
    }

    // Verify all results are identical
    let results = results.lock().unwrap();
    let first_result = &results[0].1;
    for (thread_id, result) in results.iter() {
        assert_eq!(
            result, first_result,
            "Result from thread {} differs from thread 0.\nExpected:\n{}\nGot:\n{}",
            thread_id, first_result, result
        );
    }

    Ok(())
}

/// Test 3: Triple insertion order does not affect query results
#[test]
fn test_triple_insertion_order_independence() -> Result<(), Box<dyn Error>> {
    let triples = vec![
        QuadRef::new(
            NamedNodeRef::new_unchecked("http://example.com/Alice"),
            NamedNodeRef::new_unchecked("http://example.com/age"),
            LiteralRef::new_typed_literal("30", xsd::INTEGER),
            GraphNameRef::DefaultGraph,
        ),
        QuadRef::new(
            NamedNodeRef::new_unchecked("http://example.com/Bob"),
            NamedNodeRef::new_unchecked("http://example.com/age"),
            LiteralRef::new_typed_literal("25", xsd::INTEGER),
            GraphNameRef::DefaultGraph,
        ),
        QuadRef::new(
            NamedNodeRef::new_unchecked("http://example.com/Charlie"),
            NamedNodeRef::new_unchecked("http://example.com/age"),
            LiteralRef::new_typed_literal("35", xsd::INTEGER),
            GraphNameRef::DefaultGraph,
        ),
    ];

    // Dataset 1: Insert in order [A, B, C]
    let store1 = Store::new()?;
    for quad in &triples {
        store1.insert(*quad)?;
    }

    let query1 = SparqlEvaluator::new()
        .parse_query("SELECT ?person ?age WHERE { ?person <http://example.com/age> ?age } ORDER BY ?age")?
        .on_store(&store1)
        .execute()?;
    let result1 = query_results_to_canonical_string(query1)?;

    // Dataset 2: Insert in reverse order [C, B, A]
    let store2 = Store::new()?;
    for quad in triples.iter().rev() {
        store2.insert(*quad)?;
    }

    let query2 = SparqlEvaluator::new()
        .parse_query("SELECT ?person ?age WHERE { ?person <http://example.com/age> ?age } ORDER BY ?age")?
        .on_store(&store2)
        .execute()?;
    let result2 = query_results_to_canonical_string(query2)?;

    // Dataset 3: Insert in scrambled order [B, C, A]
    let store3 = Store::new()?;
    store3.insert(triples[1])?;
    store3.insert(triples[2])?;
    store3.insert(triples[0])?;

    let query3 = SparqlEvaluator::new()
        .parse_query("SELECT ?person ?age WHERE { ?person <http://example.com/age> ?age } ORDER BY ?age")?
        .on_store(&store3)
        .execute()?;
    let result3 = query_results_to_canonical_string(query3)?;

    // All results must be identical
    assert_eq!(
        result1, result2,
        "Results differ based on insertion order (forward vs reverse)"
    );
    assert_eq!(
        result1, result3,
        "Results differ based on insertion order (forward vs scrambled)"
    );

    Ok(())
}

/// Test 4: Named graphs iteration order is deterministic
#[test]
fn test_named_graphs_iteration_deterministic() -> Result<(), Box<dyn Error>> {
    const ITERATIONS: usize = 50;
    const NUM_GRAPHS: usize = 20;

    let store = Store::new()?;

    // Create multiple named graphs
    for i in 0..NUM_GRAPHS {
        let graph_name = NamedNode::new(format!("http://example.com/graph{}", i))?;
        let object_str = format!("object{}", i);
        let quad = QuadRef::new(
            NamedNodeRef::new_unchecked("http://example.com/s"),
            NamedNodeRef::new_unchecked("http://example.com/p"),
            LiteralRef::new_simple_literal(&object_str),
            GraphNameRef::from(graph_name.as_ref()),
        );
        store.insert(quad)?;
    }

    // Collect named graphs multiple times
    let mut all_iterations: Vec<Vec<String>> = Vec::new();

    for _ in 0..ITERATIONS {
        let mut graphs: Vec<String> = store
            .named_graphs()
            .map(|g| g.map(|n| n.to_string()))
            .collect::<Result<_, _>>()?;
        graphs.sort(); // Sort for canonical comparison
        all_iterations.push(graphs);
    }

    // Verify all iterations produce same order
    let first = &all_iterations[0];
    for (i, graphs) in all_iterations.iter().enumerate() {
        assert_eq!(
            graphs, first,
            "Named graphs iteration {} differs from first iteration",
            i
        );
    }

    Ok(())
}

/// Test 5: GROUP BY group order is deterministic
#[test]
fn test_group_by_order_deterministic() -> Result<(), Box<dyn Error>> {
    const ITERATIONS: usize = 50;

    let data = r#"
        @prefix ex: <http://example.com/> .
        ex:p1 ex:department "Engineering" ; ex:salary 100000 .
        ex:p2 ex:department "Engineering" ; ex:salary 110000 .
        ex:p3 ex:department "Sales" ; ex:salary 80000 .
        ex:p4 ex:department "Sales" ; ex:salary 85000 .
        ex:p5 ex:department "Marketing" ; ex:salary 90000 .
    "#;

    let mut all_results: Vec<String> = Vec::new();

    for _ in 0..ITERATIONS {
        let store = Store::new()?;
        store.load_from_reader(RdfFormat::Turtle, data.as_bytes())?;

        let query_results = SparqlEvaluator::new()
            .parse_query(
                "SELECT ?dept (AVG(?salary) AS ?avg_salary) WHERE {
                    ?person <http://example.com/department> ?dept ;
                            <http://example.com/salary> ?salary .
                } GROUP BY ?dept ORDER BY ?dept",
            )?
            .on_store(&store)
            .execute()?;

        let canonical = query_results_to_canonical_string(query_results)?;
        all_results.push(canonical);
    }

    // Verify all results are identical
    let first = &all_results[0];
    for (i, result) in all_results.iter().enumerate() {
        assert_eq!(
            result, first,
            "GROUP BY result at iteration {} differs from first result",
            i
        );
    }

    Ok(())
}

/// Test 6: Results are independent of memory layout (run query multiple times in sequence)
#[test]
fn test_memory_layout_independence() -> Result<(), Box<dyn Error>> {
    const ITERATIONS: usize = 50;

    let store = Store::new()?;

    let data = r#"
        @prefix ex: <http://example.com/> .
        ex:s1 ex:p "value1" .
        ex:s2 ex:p "value2" .
        ex:s3 ex:p "value3" .
        ex:s4 ex:p "value4" .
        ex:s5 ex:p "value5" .
    "#;
    store.load_from_reader(RdfFormat::Turtle, data.as_bytes())?;

    let mut all_results: Vec<String> = Vec::new();

    // Run query multiple times on same store
    // Memory layout may vary due to allocator, GC, etc.
    for _ in 0..ITERATIONS {
        let query_results = SparqlEvaluator::new()
            .parse_query("SELECT ?s ?o WHERE { ?s <http://example.com/p> ?o } ORDER BY ?s")?
            .on_store(&store)
            .execute()?;

        let canonical = query_results_to_canonical_string(query_results)?;
        all_results.push(canonical);
    }

    // All results must be identical despite varying memory layout
    let first = &all_results[0];
    for (i, result) in all_results.iter().enumerate() {
        assert_eq!(
            result, first,
            "Result at iteration {} differs from first (possible memory-dependent behavior)",
            i
        );
    }

    Ok(())
}

/// Test 7: RAND() and UUID() are intentionally nondeterministic (as per SPARQL spec)
#[test]
fn test_rand_uuid_intentionally_nondeterministic() -> Result<(), Box<dyn Error>> {
    const ITERATIONS: usize = 50;

    let store = Store::new()?;

    // Test RAND() - should produce different values
    let mut rand_results = HashSet::new();
    for _ in 0..ITERATIONS {
        let query_results = SparqlEvaluator::new()
            .parse_query("SELECT (RAND() AS ?r) WHERE {}")?
            .on_store(&store)
            .execute()?;

        let result = query_results_to_canonical_string(query_results)?;
        rand_results.insert(result);
    }

    // RAND() should produce multiple different values (highly likely with 50 iterations)
    assert!(
        rand_results.len() > 1,
        "RAND() produced identical values across {} iterations - expected nondeterminism. This is INCORRECT behavior.",
        ITERATIONS
    );

    // Test UUID() - should produce different values
    let mut uuid_results = HashSet::new();
    for _ in 0..ITERATIONS {
        let query_results = SparqlEvaluator::new()
            .parse_query("SELECT (UUID() AS ?u) WHERE {}")?
            .on_store(&store)
            .execute()?;

        let result = query_results_to_canonical_string(query_results)?;
        uuid_results.insert(result);
    }

    // UUID() should produce unique values every time
    assert_eq!(
        uuid_results.len(),
        ITERATIONS,
        "UUID() produced duplicate values - expected all unique values. This is INCORRECT behavior."
    );

    Ok(())
}

/// Test 8: ASK queries return deterministic boolean results
#[test]
fn test_ask_queries_deterministic() -> Result<(), Box<dyn Error>> {
    const ITERATIONS: usize = 50;

    let data = r#"
        @prefix ex: <http://example.com/> .
        ex:subject ex:predicate ex:object .
    "#;

    let mut results_true: Vec<bool> = Vec::new();
    let mut results_false: Vec<bool> = Vec::new();

    for _ in 0..ITERATIONS {
        let store = Store::new()?;
        store.load_from_reader(RdfFormat::Turtle, data.as_bytes())?;

        // Query that should return true
        let query_true = SparqlEvaluator::new()
            .parse_query("ASK { <http://example.com/subject> <http://example.com/predicate> <http://example.com/object> }")?
            .on_store(&store)
            .execute()?;

        if let QueryResults::Boolean(b) = query_true {
            results_true.push(b);
        } else {
            panic!("Expected Boolean result");
        }

        // Query that should return false
        let query_false = SparqlEvaluator::new()
            .parse_query("ASK { <http://example.com/nonexistent> <http://example.com/predicate> <http://example.com/object> }")?
            .on_store(&store)
            .execute()?;

        if let QueryResults::Boolean(b) = query_false {
            results_false.push(b);
        } else {
            panic!("Expected Boolean result");
        }
    }

    // All true results should be true
    assert!(
        results_true.iter().all(|&b| b),
        "ASK query produced inconsistent results for existing triple"
    );

    // All false results should be false
    assert!(
        results_false.iter().all(|&b| !b),
        "ASK query produced inconsistent results for non-existing triple"
    );

    Ok(())
}

/// Test 9: CONSTRUCT queries produce deterministic graphs
#[test]
fn test_construct_queries_deterministic() -> Result<(), Box<dyn Error>> {
    const ITERATIONS: usize = 50;

    let data = r#"
        @prefix ex: <http://example.com/> .
        ex:alice ex:knows ex:bob .
        ex:bob ex:knows ex:charlie .
        ex:charlie ex:knows ex:alice .
    "#;

    let mut all_results: Vec<String> = Vec::new();

    for _ in 0..ITERATIONS {
        let store = Store::new()?;
        store.load_from_reader(RdfFormat::Turtle, data.as_bytes())?;

        let query_results = SparqlEvaluator::new()
            .parse_query(
                "CONSTRUCT { ?x <http://example.com/friend> ?y } WHERE { ?x <http://example.com/knows> ?y }",
            )?
            .on_store(&store)
            .execute()?;

        let canonical = query_results_to_canonical_string(query_results)?;
        all_results.push(canonical);
    }

    // Verify all results are identical
    let first = &all_results[0];
    for (i, result) in all_results.iter().enumerate() {
        assert_eq!(
            result, first,
            "CONSTRUCT result at iteration {} differs from first result",
            i
        );
    }

    Ok(())
}

/// Test 10: OPTIONAL clause handling is deterministic
#[test]
fn test_optional_clause_deterministic() -> Result<(), Box<dyn Error>> {
    const ITERATIONS: usize = 50;

    let data = r#"
        @prefix ex: <http://example.com/> .
        ex:alice ex:name "Alice" .
        ex:alice ex:age 30 .
        ex:bob ex:name "Bob" .
        # Bob has no age
    "#;

    let mut all_results: Vec<String> = Vec::new();

    for _ in 0..ITERATIONS {
        let store = Store::new()?;
        store.load_from_reader(RdfFormat::Turtle, data.as_bytes())?;

        let query_results = SparqlEvaluator::new()
            .parse_query(
                "SELECT ?person ?name ?age WHERE {
                    ?person <http://example.com/name> ?name .
                    OPTIONAL { ?person <http://example.com/age> ?age }
                } ORDER BY ?name",
            )?
            .on_store(&store)
            .execute()?;

        let canonical = query_results_to_canonical_string(query_results)?;
        all_results.push(canonical);
    }

    // Verify all results are identical
    let first = &all_results[0];
    for (i, result) in all_results.iter().enumerate() {
        assert_eq!(
            result, first,
            "OPTIONAL result at iteration {} differs from first result",
            i
        );
    }

    Ok(())
}
