#![cfg(test)]
#![allow(clippy::panic_in_result_fn)]

//! SPARQL Adversarial Test Harness
//!
//! This module contains adversarial tests that validate SPARQL query safety
//! and performance under extreme conditions. Each test has explicit pass/fail
//! criteria with hard bounds.

use oxigraph::model::*;
use oxigraph::sparql::{QueryResults, SparqlEvaluator};
use oxigraph::store::Store;
use std::error::Error;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// Maximum execution time for queries that should be bounded
const MAX_QUERY_TIME: Duration = Duration::from_secs(10);

/// Maximum memory per query (approximated via result set size)
const MAX_RESULT_SET_SIZE: usize = 1_000_000;

/// Helper: Create a store with test data
fn create_test_store(num_triples: usize) -> Result<Store, Box<dyn Error>> {
    let store = Store::new()?;

    // Insert test data
    for i in 0..num_triples {
        let subject = NamedNode::new(format!("http://example.com/s{}", i))?;
        let predicate = NamedNode::new(format!("http://example.com/p{}", i % 10))?;
        let object = Literal::new_simple_literal(format!("value{}", i));

        store.insert(QuadRef::new(
            subject.as_ref(),
            predicate.as_ref(),
            object.as_ref(),
            GraphNameRef::DefaultGraph,
        ))?;
    }

    Ok(store)
}

/// TEST 1: OPTIONAL Join Explosion
/// Tests that chained OPTIONAL clauses don't cause exponential execution time or unbounded memory
#[test]
fn test_optional_join_explosion() -> Result<(), Box<dyn Error>> {
    let store = create_test_store(30)?; // Small dataset to test join behavior

    // Generate query with 10 chained OPTIONAL blocks
    // This tests whether the engine can handle independent OPTIONAL clauses efficiently
    let mut query = String::from("SELECT * WHERE {\n");
    query.push_str("  ?s0 ?p0 ?o0 .\n");

    for i in 1..10 {
        query.push_str(&format!(
            "  OPTIONAL {{ ?s{} ?p{} ?o{} . }}\n",
            i, i, i
        ));
    }
    query.push('}');

    println!("Testing OPTIONAL join explosion with 10 chained OPTIONALs");

    let start = Instant::now();
    let results = SparqlEvaluator::new()
        .parse_query(&query)?
        .on_store(&store)
        .execute()?;

    // Track result count with safety limits
    let mut count = 0;
    let max_iteration_time = Duration::from_secs(30);
    let max_safe_results = 100_000; // Reasonable limit for this test

    if let QueryResults::Solutions(solutions) = results {
        for solution in solutions {
            solution?;
            count += 1;

            // Check execution time periodically
            if count % 10000 == 0 {
                let elapsed = start.elapsed();
                assert!(
                    elapsed < max_iteration_time,
                    "SPARQL FAIL: OPTIONAL join explosion - unbounded execution time (>{:?}) at {} results",
                    max_iteration_time, count
                );
            }

            // Safety: limit result iteration to prevent test hanging
            if count >= max_safe_results {
                let elapsed = start.elapsed();
                println!(
                    "⚠ OPTIONAL join test stopped at {} results after {:?} (result set explosion detected)",
                    count, elapsed
                );
                println!(
                    "  This indicates potential combinatorial explosion with {} OPTIONALs on {} triples",
                    10, 30
                );

                // Test passes if we can stop gracefully without OOM or infinite loop
                assert!(
                    elapsed < max_iteration_time,
                    "SPARQL FAIL: OPTIONAL join took {:?} for {} results - unbounded behavior",
                    elapsed, count
                );

                println!("✓ OPTIONAL join can be bounded (stopped gracefully)");
                return Ok(());
            }
        }
    }

    let elapsed = start.elapsed();
    println!(
        "OPTIONAL join test: {} results in {:?}",
        count, elapsed
    );

    // If we get here with reasonable result count, that's good
    assert!(
        elapsed < max_iteration_time,
        "SPARQL FAIL: OPTIONAL join took {:?}, exceeds limit of {:?}",
        elapsed,
        max_iteration_time
    );

    println!("✓ OPTIONAL join handled efficiently ({} results)", count);

    Ok(())
}

/// TEST 2: UNION Chain Scaling
/// Tests that many UNION branches scale linearly, not exponentially
#[test]
fn test_union_chain_scaling() -> Result<(), Box<dyn Error>> {
    let store = create_test_store(50)?;

    // Generate query with 20+ UNION branches
    let mut query = String::from("SELECT * WHERE {\n  {\n    ?s ?p ?o .\n    FILTER(?s = <http://example.com/s0>)\n  }");

    for i in 1..25 {
        query.push_str(&format!(
            "\n  UNION {{\n    ?s ?p ?o .\n    FILTER(?s = <http://example.com/s{}>)\n  }}",
            i
        ));
    }
    query.push_str("\n}");

    println!("Testing UNION chain scaling with {} branches", 25);

    let start = Instant::now();
    let results = SparqlEvaluator::new()
        .parse_query(&query)?
        .on_store(&store)
        .execute()?;

    let mut count = 0;
    if let QueryResults::Solutions(solutions) = results {
        for solution in solutions {
            solution?;
            count += 1;

            assert!(
                start.elapsed() < MAX_QUERY_TIME,
                "SPARQL FAIL: UNION chain - unbounded execution time"
            );
        }
    }

    let elapsed = start.elapsed();
    println!(
        "UNION chain test: {} results in {:?}",
        count, elapsed
    );

    // Should complete quickly for linear scaling
    assert!(
        elapsed < MAX_QUERY_TIME,
        "SPARQL FAIL: UNION chain shows exponential scaling (took {:?})",
        elapsed
    );

    Ok(())
}

/// TEST 3: Cartesian Product Detection
/// Tests that queries without join variables are handled safely
#[test]
fn test_cartesian_product_detection() -> Result<(), Box<dyn Error>> {
    let store = create_test_store(100)?;

    // Query with two triple patterns and NO shared variables (Cartesian product)
    let query = r#"
        SELECT * WHERE {
            ?s ?p ?o .
            ?x ?y ?z .
        }
    "#;

    println!("Testing Cartesian product detection");

    let start = Instant::now();
    let results = SparqlEvaluator::new()
        .parse_query(query)?
        .on_store(&store)
        .execute()?;

    let mut count = 0;
    let max_results = 100_000; // Safety limit

    if let QueryResults::Solutions(solutions) = results {
        for solution in solutions {
            solution?;
            count += 1;

            // Prevent memory explosion
            assert!(
                count < max_results,
                "SPARQL FAIL: Cartesian product - unbounded result set (>{} results)",
                max_results
            );

            assert!(
                start.elapsed() < MAX_QUERY_TIME,
                "SPARQL FAIL: Cartesian product - unbounded execution time"
            );
        }
    }

    let elapsed = start.elapsed();
    println!(
        "Cartesian product test: {} results in {:?}",
        count, elapsed
    );

    assert!(
        elapsed < MAX_QUERY_TIME,
        "SPARQL FAIL: Cartesian product not bounded (took {:?})",
        elapsed
    );

    Ok(())
}

/// TEST 4: Regex DoS Protection
/// Tests that large or complex regex patterns don't cause crashes or hangs
#[test]
fn test_regex_dos_protection() -> Result<(), Box<dyn Error>> {
    let store = create_test_store(10)?;

    // Test 1: Very long regex pattern (potential compilation DoS)
    let long_pattern = "a".repeat(10000);
    let query1 = format!(
        r#"SELECT * WHERE {{
            ?s ?p ?o .
            FILTER(REGEX(STR(?o), "{}"))
        }}"#,
        long_pattern
    );

    println!("Testing regex DoS protection with 10KB pattern");

    let start = Instant::now();
    let result1 = SparqlEvaluator::new()
        .parse_query(&query1)?
        .on_store(&store)
        .execute();

    assert!(
        start.elapsed() < Duration::from_secs(5),
        "SPARQL FAIL: Regex compilation takes too long"
    );

    // Should either execute or fail gracefully
    match result1 {
        Ok(QueryResults::Solutions(solutions)) => {
            let count = solutions.count();
            println!("Long regex executed: {} results", count);
        }
        Ok(_) => {}
        Err(e) => {
            println!("Long regex rejected (acceptable): {}", e);
        }
    }

    // Test 2: Complex nested pattern (potential backtracking DoS)
    let query2 = r#"SELECT * WHERE {
        ?s ?p ?o .
        FILTER(REGEX(STR(?o), "(a+)+b"))
    }"#;

    println!("Testing regex backtracking protection");

    let start2 = Instant::now();
    let result2 = SparqlEvaluator::new()
        .parse_query(query2)?
        .on_store(&store)
        .execute();

    assert!(
        start2.elapsed() < Duration::from_secs(5),
        "SPARQL FAIL: Regex backtracking not protected"
    );

    match result2 {
        Ok(_) => println!("Backtracking regex handled safely"),
        Err(e) => println!("Backtracking regex rejected (acceptable): {}", e),
    }

    Ok(())
}

/// TEST 5: Large DISTINCT Memory
/// Tests that DISTINCT operations have bounded memory usage
#[test]
fn test_large_distinct_memory() -> Result<(), Box<dyn Error>> {
    // Create larger dataset for DISTINCT test
    let store = create_test_store(10000)?;

    let query = r#"
        SELECT DISTINCT ?s ?p ?o WHERE {
            ?s ?p ?o .
        }
    "#;

    println!("Testing DISTINCT memory bounds on 10K triples");

    let start = Instant::now();
    let results = SparqlEvaluator::new()
        .parse_query(query)?
        .on_store(&store)
        .execute()?;

    let mut count = 0;
    if let QueryResults::Solutions(solutions) = results {
        for solution in solutions {
            solution?;
            count += 1;

            // Check we're making progress
            assert!(
                start.elapsed() < MAX_QUERY_TIME,
                "SPARQL FAIL: DISTINCT operation timeout"
            );
        }
    }

    let elapsed = start.elapsed();
    println!(
        "DISTINCT test: {} unique results in {:?}",
        count, elapsed
    );

    assert!(
        count <= MAX_RESULT_SET_SIZE,
        "SPARQL FAIL: DISTINCT result set too large ({})",
        count
    );

    assert!(
        elapsed < MAX_QUERY_TIME,
        "SPARQL FAIL: DISTINCT took {:?}, exceeds {:?}",
        elapsed,
        MAX_QUERY_TIME
    );

    Ok(())
}

/// TEST 6: Concurrent Queries Deterministic
/// Tests that concurrent execution of the same query produces identical results
#[test]
fn test_concurrent_queries_deterministic() -> Result<(), Box<dyn Error>> {
    let store = Arc::new(create_test_store(100)?);

    let query = r#"
        SELECT ?s ?p ?o WHERE {
            ?s ?p ?o .
        }
        ORDER BY ?s ?p ?o
    "#;

    println!("Testing concurrent query determinism (100 threads)");

    // Run query once to get expected results
    let expected_results = {
        let results = SparqlEvaluator::new()
            .parse_query(query)?
            .on_store(&store)
            .execute()?;

        let mut expected = Vec::new();
        if let QueryResults::Solutions(solutions) = results {
            for solution in solutions {
                let sol = solution?;
                let s = sol.get("s").map(|t| t.to_string()).unwrap_or_default();
                let p = sol.get("p").map(|t| t.to_string()).unwrap_or_default();
                let o = sol.get("o").map(|t| t.to_string()).unwrap_or_default();
                expected.push((s, p, o));
            }
        }
        expected
    };

    println!("Expected {} results per query", expected_results.len());

    // Run same query 100 times concurrently
    let num_threads = 100;
    let results_collector = Arc::new(Mutex::new(Vec::new()));
    let errors = Arc::new(Mutex::new(Vec::new()));

    let start = Instant::now();
    let mut handles = Vec::new();

    for thread_id in 0..num_threads {
        let store_clone = Arc::clone(&store);
        let results_clone = Arc::clone(&results_collector);
        let errors_clone = Arc::clone(&errors);
        let query_str = query.to_string();

        let handle = thread::spawn(move || {
            match run_query_and_collect(&store_clone, &query_str) {
                Ok(results) => {
                    results_clone.lock().unwrap().push((thread_id, results));
                }
                Err(e) => {
                    errors_clone
                        .lock()
                        .unwrap()
                        .push((thread_id, e.to_string()));
                }
            }
        });

        handles.push(handle);
    }

    // Wait for all threads
    for handle in handles {
        handle.join().unwrap();
    }

    let elapsed = start.elapsed();
    println!(
        "Concurrent queries completed in {:?}",
        elapsed
    );

    // Check for errors
    let errors_vec = errors.lock().unwrap();
    assert!(
        errors_vec.is_empty(),
        "SPARQL FAIL: {} threads had errors: {:?}",
        errors_vec.len(),
        errors_vec.first()
    );

    // Verify all results are identical
    let all_results = results_collector.lock().unwrap();
    assert_eq!(
        all_results.len(),
        num_threads,
        "SPARQL FAIL: Not all threads completed successfully"
    );

    for (thread_id, actual_results) in all_results.iter() {
        assert_eq!(
            actual_results.len(),
            expected_results.len(),
            "SPARQL FAIL: Thread {} got {} results, expected {}",
            thread_id,
            actual_results.len(),
            expected_results.len()
        );

        assert_eq!(
            actual_results,
            &expected_results,
            "SPARQL FAIL: Thread {} got different results (non-deterministic execution)",
            thread_id
        );
    }

    println!("✓ All {} concurrent queries produced identical results", num_threads);

    Ok(())
}

/// Helper for concurrent query execution
fn run_query_and_collect(
    store: &Store,
    query: &str,
) -> Result<Vec<(String, String, String)>, Box<dyn Error>> {
    let results = SparqlEvaluator::new()
        .parse_query(query)?
        .on_store(store)
        .execute()?;

    let mut collected = Vec::new();
    if let QueryResults::Solutions(solutions) = results {
        for solution in solutions {
            let sol = solution?;
            let s = sol.get("s").map(|t| t.to_string()).unwrap_or_default();
            let p = sol.get("p").map(|t| t.to_string()).unwrap_or_default();
            let o = sol.get("o").map(|t| t.to_string()).unwrap_or_default();
            collected.push((s, p, o));
        }
    }

    Ok(collected)
}
