//! OWL Reasoning Bounds and Safety Tests
//!
//! This test suite verifies critical safety properties and bounds checking
//! in the OWL 2 RL reasoner. Tests are designed to FAIL if safety issues exist.
//!
//! **CRITICAL FINDINGS UNDER TEST:**
//! 1. Iteration limit hit silently (returns incomplete results)
//! 2. No timeout enforcement
//! 3. No memory limits
//! 4. No profile validation (accepts non-RL ontologies)
//! 5. No entailment explanation
//!
//! Run with: cargo test -p oxowl owl_reasoning_bounds

#![cfg(feature = "reasoner-rl")]

use oxowl::{
    Axiom, ClassExpression, Individual, ObjectProperty, Ontology, OwlClass, Reasoner,
    ReasonerConfig, RlReasoner,
};
use oxrdf::NamedNode;
use std::time::{Duration, Instant};

/// Test 1: BLOCKER - Iteration limit hit silently
///
/// EXPECTED BEHAVIOR: When iteration limit is reached, reasoner should:
/// - Return an error indicating incomplete reasoning
/// - OR set a flag indicating results may be incomplete
/// - OR log a warning
///
/// ACTUAL BEHAVIOR: Returns Ok(()) with incomplete results (SILENT FAILURE)
///
/// IMPACT: Operators cannot trust completeness of reasoning results
#[test]
fn test_owl_reasoning_iteration_limit_detected() {
    println!("\n=== Test 1: Iteration Limit Detection ===");

    let mut ontology = Ontology::with_iri("http://example.org/iteration-test").unwrap();

    // Create a deep class hierarchy that requires many iterations
    // A0 ⊑ A1 ⊑ A2 ⊑ ... ⊑ A999
    let classes: Vec<OwlClass> = (0..1000)
        .map(|i| {
            OwlClass::new(
                NamedNode::new(format!("http://example.org/Class{}", i)).unwrap(),
            )
        })
        .collect();

    // Build linear chain
    for i in 0..999 {
        ontology.add_axiom(Axiom::subclass_of(
            ClassExpression::class(classes[i].clone()),
            ClassExpression::class(classes[i + 1].clone()),
        ));
    }

    // Create transitive property with deep chain
    let has_ancestor =
        ObjectProperty::new(NamedNode::new("http://example.org/hasAncestor").unwrap());
    ontology.add_axiom(Axiom::TransitiveObjectProperty(has_ancestor.clone()));

    // Create 100 individuals in a chain
    let individuals: Vec<Individual> = (0..100)
        .map(|i| {
            Individual::Named(
                NamedNode::new(format!("http://example.org/ind{}", i)).unwrap(),
            )
        })
        .collect();

    // Chain: ind0 -> ind1 -> ind2 -> ... -> ind99
    for i in 0..99 {
        ontology.add_axiom(Axiom::ObjectPropertyAssertion {
            property: has_ancestor.clone(),
            source: individuals[i].clone(),
            target: individuals[i + 1].clone(),
        });
    }

    println!("Created ontology with:");
    println!("  - {} classes in linear hierarchy", classes.len());
    println!("  - {} individuals in transitive chain", individuals.len());
    println!("  - Expected iterations: >1000");

    // Set artificially low iteration limit
    let config = ReasonerConfig {
        max_iterations: 10, // Very low to force limit hit
        check_consistency: true,
        materialize: true,
    };

    let mut reasoner = RlReasoner::with_config(&ontology, config);
    let result = reasoner.classify();

    // CRITICAL CHECK: Did the reasoner indicate the limit was hit?
    match result {
        Ok(()) => {
            // Reasoner returned Ok - this is SILENT FAILURE
            let inferred_axioms = reasoner.get_inferred_axioms();
            println!(
                "\n❌ BLOCKER CONFIRMED: Iteration limit hit silently!"
            );
            println!("  - Reasoner returned: Ok(())");
            println!("  - Inferred axioms: {}", inferred_axioms.len());
            println!("  - No error or warning about incomplete reasoning");

            // Check if reasoning was actually incomplete
            let superclasses = reasoner.get_super_classes(&classes[0], false);
            let expected_superclasses = 999; // Should infer all 999 superclasses
            let actual_superclasses = superclasses.len();

            println!(
                "  - Expected superclasses for Class0: {}",
                expected_superclasses
            );
            println!(
                "  - Actual superclasses for Class0: {}",
                actual_superclasses
            );

            if actual_superclasses < expected_superclasses {
                println!("  - Reasoning is INCOMPLETE (only {}% complete)",
                    (actual_superclasses * 100) / expected_superclasses
                );
            }

            // This test should FAIL to highlight the blocker
            panic!(
                "BLOCKER: Iteration limit hit silently! \
                Reasoner should return error or set incomplete flag, but returned Ok(())"
            );
        }
        Err(e) => {
            // Good! Reasoner properly reported the issue
            println!("✓ PASS: Reasoner correctly reported iteration limit");
            println!("  - Error: {:?}", e);
        }
    }
}

/// Test 2: BLOCKER - No timeout enforcement
///
/// EXPECTED BEHAVIOR: ReasonerConfig should have timeout parameter
/// ACTUAL BEHAVIOR: No timeout field exists (MISSING SAFETY FEATURE)
///
/// IMPACT: Long-running reasoning cannot be bounded, potential DoS
#[test]
fn test_owl_reasoning_timeout_enforcement() {
    println!("\n=== Test 2: Timeout Enforcement ===");

    // Check if ReasonerConfig has timeout field
    let config = ReasonerConfig {
        max_iterations: 100_000,
        check_consistency: true,
        materialize: true,
        // Attempt to set timeout - this will fail to compile if field doesn't exist
        // timeout: Duration::from_secs(5),
    };

    println!("ReasonerConfig fields:");
    println!("  - max_iterations: {}", config.max_iterations);
    println!("  - check_consistency: {}", config.check_consistency);
    println!("  - materialize: {}", config.materialize);
    println!("  - timeout: NOT AVAILABLE");

    println!("\n❌ BLOCKER CONFIRMED: No timeout enforcement!");
    println!("  - ReasonerConfig has no timeout field");
    println!("  - Long-running reasoning cannot be bounded");
    println!("  - Potential DoS vector");

    // Create a potentially long-running reasoning task
    let mut ontology = Ontology::with_iri("http://example.org/timeout-test").unwrap();

    // Create a scenario that could take long time
    // Multiple transitive properties with deep chains
    for prop_id in 0..10 {
        let property = ObjectProperty::new(
            NamedNode::new(format!("http://example.org/prop{}", prop_id)).unwrap(),
        );
        ontology.add_axiom(Axiom::TransitiveObjectProperty(property.clone()));

        // Create chain for each property
        for i in 0..50 {
            let ind_a = Individual::Named(
                NamedNode::new(format!("http://example.org/p{}_ind{}", prop_id, i))
                    .unwrap(),
            );
            let ind_b = Individual::Named(
                NamedNode::new(format!("http://example.org/p{}_ind{}", prop_id, i + 1))
                    .unwrap(),
            );
            ontology.add_axiom(Axiom::ObjectPropertyAssertion {
                property: property.clone(),
                source: ind_a,
                target: ind_b,
            });
        }
    }

    let mut reasoner = RlReasoner::new(&ontology);
    let start = Instant::now();
    let _result = reasoner.classify();
    let elapsed = start.elapsed();

    println!("\nReasoning completed in: {:?}", elapsed);
    println!("  - No timeout was enforced");
    println!("  - Reasoning ran to completion or hit iteration limit");

    // This test documents the blocker
    panic!(
        "BLOCKER: No timeout enforcement! \
        ReasonerConfig should have timeout field for safety"
    );
}

/// Test 3: BLOCKER - No memory limits
///
/// EXPECTED BEHAVIOR: Reasoner should monitor memory and stop if limit exceeded
/// ACTUAL BEHAVIOR: No memory monitoring or limits (RESOURCE EXHAUSTION RISK)
///
/// IMPACT: Malicious ontologies can cause OOM, crash the system
#[test]
fn test_owl_memory_bounded() {
    println!("\n=== Test 3: Memory Bounds ===");

    let mut ontology = Ontology::with_iri("http://example.org/memory-test").unwrap();

    // Create ontology that generates many inferred axioms
    // This creates a cartesian product effect

    // Create 100 classes
    let classes: Vec<OwlClass> = (0..100)
        .map(|i| {
            OwlClass::new(
                NamedNode::new(format!("http://example.org/C{}", i)).unwrap(),
            )
        })
        .collect();

    // Make them all equivalent (creates O(n²) inferred axioms)
    ontology.add_axiom(Axiom::equivalent_classes(
        classes
            .iter()
            .map(|c| ClassExpression::class(c.clone()))
            .collect(),
    ));

    // Create 500 individuals
    let individuals: Vec<Individual> = (0..500)
        .map(|i| {
            Individual::Named(
                NamedNode::new(format!("http://example.org/i{}", i)).unwrap(),
            )
        })
        .collect();

    // Assert each individual is of each class (500 * 100 = 50,000 assertions)
    for individual in &individuals {
        for class in &classes {
            ontology.add_axiom(Axiom::class_assertion(
                ClassExpression::class(class.clone()),
                individual.clone(),
            ));
        }
    }

    println!("Created memory-intensive ontology:");
    println!("  - {} equivalent classes", classes.len());
    println!("  - {} individuals", individuals.len());
    println!("  - {} initial assertions", individuals.len() * classes.len());
    println!(
        "  - Expected inferred axioms: >{}",
        individuals.len() * classes.len()
    );

    let config = ReasonerConfig {
        max_iterations: 100_000,
        check_consistency: true,
        materialize: true,
        // Attempt to set memory limit - field doesn't exist
        // max_memory_mb: 100,
    };

    println!("\n❌ BLOCKER CONFIRMED: No memory limits!");
    println!("  - ReasonerConfig has no max_memory field");
    println!("  - No memory monitoring during reasoning");
    println!("  - Risk of OOM on large ontologies");

    let mut reasoner = RlReasoner::with_config(&ontology, config);
    let _result = reasoner.classify();

    let inferred = reasoner.get_inferred_axioms();
    println!("\nReasoning completed:");
    println!("  - Inferred axioms: {}", inferred.len());
    println!("  - No memory limit was enforced");

    // This test documents the blocker
    panic!(
        "BLOCKER: No memory limits! \
        Reasoner should have configurable memory bounds to prevent OOM"
    );
}

/// Test 4: BLOCKER - No OWL 2 RL profile validation
///
/// EXPECTED BEHAVIOR: Reasoner should validate ontology is OWL 2 RL compliant
/// ACTUAL BEHAVIOR: Accepts any ontology, silently ignores non-RL features
///
/// IMPACT: Operators get incomplete reasoning without knowing why
#[test]
fn test_owl_rl_profile_enforcement() {
    println!("\n=== Test 4: OWL 2 RL Profile Validation ===");

    // Create ontology with OWL 2 DL features (not in RL profile)
    let mut ontology = Ontology::with_iri("http://example.org/profile-test").unwrap();

    let animal = OwlClass::new(NamedNode::new("http://example.org/Animal").unwrap());
    let plant = OwlClass::new(NamedNode::new("http://example.org/Plant").unwrap());

    // OWL 2 DL feature: Universal restriction (∀) on LHS
    // This is NOT in OWL 2 RL profile!
    let has_part =
        ObjectProperty::new(NamedNode::new("http://example.org/hasPart").unwrap());

    // ∀hasPart.Animal ⊑ Plant  (universal on LHS - not in RL!)
    ontology.add_axiom(Axiom::subclass_of(
        ClassExpression::all_values_from(
            has_part.clone(),
            ClassExpression::class(animal.clone()),
        ),
        ClassExpression::class(plant.clone()),
    ));

    // OWL 2 DL feature: Existential restriction (∃) on RHS
    // This is also NOT standard in OWL 2 RL!
    let living_thing =
        OwlClass::new(NamedNode::new("http://example.org/LivingThing").unwrap());

    // Plant ⊑ ∃hasPart.Animal  (existential on RHS - limited in RL!)
    ontology.add_axiom(Axiom::subclass_of(
        ClassExpression::class(plant.clone()),
        ClassExpression::some_values_from(
            has_part,
            ClassExpression::class(animal.clone()),
        ),
    ));

    // OWL 2 DL feature: Complement (¬) in subclass position
    // Negation is very restricted in OWL 2 RL
    ontology.add_axiom(Axiom::subclass_of(
        ClassExpression::complement(ClassExpression::class(animal.clone())),
        ClassExpression::class(plant.clone()),
    ));

    println!("Created ontology with OWL 2 DL features:");
    println!("  - Universal restriction (∀) on LHS");
    println!("  - Existential restriction (∃) on RHS");
    println!("  - Complement (¬) in subclass position");
    println!("\nThese features are NOT in OWL 2 RL profile!");

    let mut reasoner = RlReasoner::new(&ontology);
    let result = reasoner.classify();

    match result {
        Ok(()) => {
            println!("\n❌ BLOCKER CONFIRMED: No profile validation!");
            println!("  - Reasoner accepted non-RL ontology");
            println!("  - Returned: Ok(())");
            println!("  - No warning about unsupported features");
            println!("  - Silently ignores non-RL axioms");
            println!("\nIMPACT:");
            println!("  - Operators get incomplete reasoning");
            println!("  - No indication why inferences are missing");
            println!("  - Cannot trust completeness");

            panic!(
                "BLOCKER: No OWL 2 RL profile validation! \
                Reasoner should reject or warn about non-RL features"
            );
        }
        Err(e) => {
            println!("✓ PASS: Reasoner correctly rejected non-RL ontology");
            println!("  - Error: {:?}", e);
        }
    }
}

/// Test 5: Class hierarchy explosion detection
///
/// Tests whether reasoner can detect and handle exponential inference growth
#[test]
fn test_owl_class_hierarchy_explosion_detected() {
    println!("\n=== Test 5: Class Hierarchy Explosion Detection ===");

    let mut ontology = Ontology::with_iri("http://example.org/explosion-test").unwrap();

    // Create multiple inheritance hierarchy that causes explosion
    // Each level doubles the inferred relationships

    let levels = 10;
    let mut previous_level: Vec<OwlClass> = vec![];

    for level in 0..levels {
        let classes_in_level = 2_usize.pow(level);
        let mut current_level: Vec<OwlClass> = vec![];

        for i in 0..classes_in_level {
            let class = OwlClass::new(
                NamedNode::new(format!("http://example.org/L{}C{}", level, i))
                    .unwrap(),
            );
            current_level.push(class.clone());

            // Make this class a subclass of multiple parents (diamond inheritance)
            if !previous_level.is_empty() {
                let parent1_idx = i / 2;
                let parent2_idx = (i / 2 + 1) % previous_level.len();

                ontology.add_axiom(Axiom::subclass_of(
                    ClassExpression::class(class.clone()),
                    ClassExpression::class(previous_level[parent1_idx].clone()),
                ));

                if parent1_idx != parent2_idx {
                    ontology.add_axiom(Axiom::subclass_of(
                        ClassExpression::class(class.clone()),
                        ClassExpression::class(previous_level[parent2_idx].clone()),
                    ));
                }
            }
        }

        previous_level = current_level;
    }

    // Add transitive property with instances at each level
    let has_ancestor =
        ObjectProperty::new(NamedNode::new("http://example.org/hasAncestor").unwrap());
    ontology.add_axiom(Axiom::TransitiveObjectProperty(has_ancestor.clone()));

    // Create individuals in exponential pattern
    for i in 0..100 {
        let ind_a = Individual::Named(
            NamedNode::new(format!("http://example.org/ind{}", i)).unwrap(),
        );
        let ind_b = Individual::Named(
            NamedNode::new(format!("http://example.org/ind{}", i + 1)).unwrap(),
        );

        ontology.add_axiom(Axiom::ObjectPropertyAssertion {
            property: has_ancestor.clone(),
            source: ind_a,
            target: ind_b,
        });
    }

    let total_classes = (2_usize.pow(levels) - 1);
    println!("Created explosive ontology:");
    println!("  - {} levels of hierarchy", levels);
    println!("  - {} total classes", total_classes);
    println!("  - Diamond inheritance pattern");
    println!("  - Expected inferred facts: O(n²)");

    let mut reasoner = RlReasoner::new(&ontology);
    let start = Instant::now();
    let result = reasoner.classify();
    let elapsed = start.elapsed();

    println!("\nReasoning completed in: {:?}", elapsed);

    match result {
        Ok(()) => {
            let inferred = reasoner.get_inferred_axioms();
            println!("  - Inferred axioms: {}", inferred.len());

            if inferred.len() > 1_000_000 {
                println!("\n⚠ WARNING: Explosion detected!");
                println!("  - Over 1M inferred axioms");
                println!("  - No warning or limit enforcement");

                panic!(
                    "Potential BLOCKER: No explosion detection! \
                    Generated {} axioms with no warning",
                    inferred.len()
                );
            } else {
                println!("✓ Explosion bounded by iteration limit");
            }
        }
        Err(e) => {
            println!("  - Error: {:?}", e);
        }
    }
}

/// Test 6: FEATURE GAP - No entailment explanation
///
/// Tests whether reasoner can explain why a conclusion was derived
#[test]
fn test_owl_entailment_explanation() {
    println!("\n=== Test 6: Entailment Explanation ===");

    let mut ontology = Ontology::with_iri("http://example.org/explain-test").unwrap();

    let animal = OwlClass::new(NamedNode::new("http://example.org/Animal").unwrap());
    let dog = OwlClass::new(NamedNode::new("http://example.org/Dog").unwrap());
    let fido = Individual::Named(NamedNode::new("http://example.org/fido").unwrap());

    // Dog ⊑ Animal
    ontology.add_axiom(Axiom::subclass_of(
        ClassExpression::class(dog.clone()),
        ClassExpression::class(animal.clone()),
    ));

    // fido : Dog
    ontology.add_axiom(Axiom::class_assertion(
        ClassExpression::class(dog.clone()),
        fido.clone(),
    ));

    let mut reasoner = RlReasoner::new(&ontology);
    reasoner.classify().unwrap();

    // fido should be inferred as Animal
    let types = reasoner.get_types(&fido);
    assert!(types.contains(&&animal), "fido should be inferred as Animal");

    println!("Inference verified: fido is Animal");
    println!("\nAttempting to get explanation:");

    // Try to get explanation - this method doesn't exist!
    // let explanation = reasoner.explain_entailment(&fido, &animal);

    println!("\n❌ FEATURE GAP CONFIRMED: No entailment explanation!");
    println!("  - Reasoner has no explain_entailment() method");
    println!("  - Cannot trace derivation paths");
    println!("  - Operators cannot debug reasoning");
    println!("\nRECOMMENDATION:");
    println!("  - Add explain_entailment() API");
    println!("  - Track provenance during inference");
    println!("  - Return justification sets");

    // This documents the feature gap
    panic!(
        "FEATURE GAP: No entailment explanation! \
        Add explain_entailment() for debugging support"
    );
}

/// Test 7: Configuration bounds validation
///
/// Tests that invalid configurations are rejected
#[test]
fn test_configuration_validation() {
    println!("\n=== Test 7: Configuration Validation ===");

    // Test with zero max_iterations (invalid)
    let config = ReasonerConfig {
        max_iterations: 0,
        check_consistency: true,
        materialize: true,
    };

    let ontology = Ontology::new(None);
    let mut reasoner = RlReasoner::with_config(&ontology, config);

    println!("Testing with max_iterations = 0");

    match reasoner.classify() {
        Ok(()) => {
            println!("\n⚠ WARNING: Accepted invalid configuration!");
            println!("  - max_iterations = 0 should be rejected");
            println!("  - No validation performed");
        }
        Err(e) => {
            println!("✓ PASS: Rejected invalid configuration");
            println!("  - Error: {:?}", e);
        }
    }

    // Test with extreme max_iterations
    let config = ReasonerConfig {
        max_iterations: usize::MAX,
        check_consistency: true,
        materialize: true,
    };

    println!("\nTesting with max_iterations = usize::MAX");
    println!("  - This could lead to infinite-like loops");
    println!("  - Should have reasonable upper bound");

    let mut reasoner = RlReasoner::with_config(&ontology, config);
    let _result = reasoner.classify();

    println!("\n⚠ Configuration validation is minimal");
    println!("  - No bounds checking on max_iterations");
    println!("  - Could set to 0 or usize::MAX");
}

/// Summary test that documents all blockers
#[test]
#[ignore] // Run explicitly with --ignored
fn test_reasoning_bounds_summary() {
    println!("\n");
    println!("╔════════════════════════════════════════════════════════════════════╗");
    println!("║         OWL 2 RL REASONING - BOUNDS & SAFETY ANALYSIS             ║");
    println!("╚════════════════════════════════════════════════════════════════════╝");
    println!();
    println!("CRITICAL BLOCKERS IDENTIFIED:");
    println!();
    println!("❌ BLOCKER 1: Silent iteration limit failure");
    println!("   Location: lib/oxowl/src/reasoner/mod.rs:269-291");
    println!("   Issue: Loop exits when max_iterations reached, returns Ok(())");
    println!("   Impact: Incomplete reasoning with no error indication");
    println!("   Fix: Return Err() or set incomplete flag when limit hit");
    println!();
    println!("❌ BLOCKER 2: No timeout enforcement");
    println!("   Location: lib/oxowl/src/reasoner/mod.rs:16-24");
    println!("   Issue: ReasonerConfig has no timeout field");
    println!("   Impact: Long-running reasoning cannot be bounded (DoS risk)");
    println!("   Fix: Add timeout: Option<Duration> to ReasonerConfig");
    println!();
    println!("❌ BLOCKER 3: No memory limits");
    println!("   Location: lib/oxowl/src/reasoner/mod.rs (no monitoring)");
    println!("   Issue: No memory tracking or bounds");
    println!("   Impact: Malicious ontologies can cause OOM");
    println!("   Fix: Add max_memory_mb: Option<usize> and monitor usage");
    println!();
    println!("❌ BLOCKER 4: No OWL 2 RL profile validation");
    println!("   Location: lib/oxowl/src/reasoner/mod.rs:143-264");
    println!("   Issue: Accepts any ontology, silently ignores non-RL features");
    println!("   Impact: Incomplete reasoning without explanation");
    println!("   Fix: Add profile validator, warn on non-RL axioms");
    println!();
    println!("⚠ FEATURE GAP 5: No entailment explanation");
    println!("   Location: N/A (feature not implemented)");
    println!("   Issue: Cannot explain why inferences were derived");
    println!("   Impact: Difficult to debug reasoning issues");
    println!("   Fix: Add explain_entailment() with justification tracking");
    println!();
    println!("════════════════════════════════════════════════════════════════════");
    println!();
    println!("RECOMMENDATION: Do NOT deploy for production use until fixed");
    println!("PRIORITY: P0 - Critical safety issues");
    println!("TIMELINE: 2-3 weeks for minimal fixes");
    println!();
    println!("════════════════════════════════════════════════════════════════════");
}
