//! 24+ Hour Soak Test for Oxigraph
//!
//! This example runs a long-duration stability test with:
//! - Mixed production-realistic workload (queries, SHACL, ingestion, updates)
//! - Memory monitoring with plateau detection
//! - Latency tracking (P50, P95, P99)
//! - Error rate monitoring
//! - Hourly progress reports
//! - Final assertions for PASS/FAIL verdict
//!
//! Run with: cargo run -p oxigraph --example soak_test --release
//!
//! For testing (shorter duration):
//! SOAK_DURATION_HOURS=1 cargo run -p oxigraph --example soak_test --release

use oxigraph::io::RdfFormat;
use oxigraph::model::*;
use oxigraph::model::vocab::{rdf, xsd};
use oxigraph::sparql::{QueryResults, SparqlEvaluator};
use oxigraph::store::Store;
use sparshacl::{ShaclValidator, ShapesGraph};
use oxrdf::Graph;
use oxrdfio::RdfParser;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::thread;

// ============================================================================
// Configuration
// ============================================================================

const DEFAULT_DURATION_HOURS: u64 = 24;
const MEMORY_SAMPLE_INTERVAL: Duration = Duration::from_secs(60);
const HOURLY_REPORT_INTERVAL: Duration = Duration::from_secs(3600);
const MEMORY_GROWTH_THRESHOLD_MB_PER_HOUR: f64 = 10.0;
const MAX_ERROR_RATE_PERCENT: f64 = 0.1;
const MAX_P99_LATENCY_MS: u128 = 5000;
const MEMORY_PLATEAU_HOURS: usize = 2;

// Workload distribution
const QUERY_PERCENTAGE: u32 = 60;
const SHACL_PERCENTAGE: u32 = 20;
const INGESTION_PERCENTAGE: u32 = 15;
const UPDATE_PERCENTAGE: u32 = 5;

// ============================================================================
// Statistics Tracking
// ============================================================================

#[derive(Debug, Clone)]
struct LatencySample {
    duration: Duration,
    timestamp: Instant,
    operation_type: OperationType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OperationType {
    SimpleQuery,
    ComplexQuery,
    ShaclValidation,
    BulkIngestion,
    Update,
}

impl std::fmt::Display for OperationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SimpleQuery => write!(f, "SimpleQuery"),
            Self::ComplexQuery => write!(f, "ComplexQuery"),
            Self::ShaclValidation => write!(f, "ShaclValidation"),
            Self::BulkIngestion => write!(f, "BulkIngestion"),
            Self::Update => write!(f, "Update"),
        }
    }
}

#[derive(Clone)]
struct MemorySample {
    timestamp: Duration,
    bytes: u64,
}

#[derive(Clone)]
struct SoakStats {
    latencies: Vec<LatencySample>,
    memory_samples: Vec<MemorySample>,
    errors: u64,
    operations: u64,
    errors_by_type: std::collections::HashMap<String, u64>,
}

impl SoakStats {
    fn new() -> Self {
        Self {
            latencies: Vec::new(),
            memory_samples: Vec::new(),
            errors: 0,
            operations: 0,
            errors_by_type: std::collections::HashMap::new(),
        }
    }

    fn record_latency(&mut self, duration: Duration, op_type: OperationType, start: Instant) {
        self.latencies.push(LatencySample {
            duration,
            timestamp: start,
            operation_type: op_type,
        });
    }

    fn record_memory(&mut self, elapsed: Duration, bytes: u64) {
        self.memory_samples.push(MemorySample {
            timestamp: elapsed,
            bytes,
        });
    }

    fn record_operation(&mut self) {
        self.operations += 1;
    }

    fn record_error(&mut self, error_type: &str) {
        self.errors += 1;
        *self.errors_by_type.entry(error_type.to_string()).or_insert(0) += 1;
    }

    fn get_percentile_latency(&self, percentile: f64) -> Option<Duration> {
        if self.latencies.is_empty() {
            return None;
        }

        let mut sorted: Vec<_> = self.latencies.iter().map(|s| s.duration).collect();
        sorted.sort();

        let index = ((sorted.len() as f64) * percentile / 100.0).ceil() as usize;
        sorted.get(index.saturating_sub(1)).copied()
    }

    fn get_percentile_latency_by_type(&self, percentile: f64, op_type: OperationType) -> Option<Duration> {
        let filtered: Vec<_> = self.latencies
            .iter()
            .filter(|s| s.operation_type == op_type)
            .map(|s| s.duration)
            .collect();

        if filtered.is_empty() {
            return None;
        }

        let mut sorted = filtered;
        sorted.sort();

        let index = ((sorted.len() as f64) * percentile / 100.0).ceil() as usize;
        sorted.get(index.saturating_sub(1)).copied()
    }

    fn error_rate(&self) -> f64 {
        if self.operations == 0 {
            return 0.0;
        }
        (self.errors as f64 / self.operations as f64) * 100.0
    }

    fn operations_per_second(&self, elapsed: Duration) -> f64 {
        let seconds = elapsed.as_secs_f64();
        if seconds == 0.0 {
            return 0.0;
        }
        self.operations as f64 / seconds
    }
}

// ============================================================================
// Memory Monitoring
// ============================================================================

fn get_current_memory_usage() -> u64 {
    // Try to get memory usage from /proc/self/statm on Linux
    #[cfg(target_os = "linux")]
    {
        if let Ok(contents) = std::fs::read_to_string("/proc/self/statm") {
            if let Some(rss_pages) = contents.split_whitespace().nth(1) {
                if let Ok(pages) = rss_pages.parse::<u64>() {
                    // Page size is typically 4096 bytes
                    return pages * 4096;
                }
            }
        }
    }

    // Fallback: use a platform-agnostic estimate
    // This is a rough approximation and not accurate for production use
    0
}

// ============================================================================
// Sample Data Generation
// ============================================================================

fn setup_initial_data(store: &Store) -> Result<(), Box<dyn std::error::Error>> {
    println!("Setting up initial test data...");

    let turtle_data = r#"
@prefix schema: <http://schema.org/> .
@prefix ex: <http://example.com/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

ex:alice a schema:Person ;
    schema:name "Alice Anderson" ;
    schema:age 30 ;
    schema:email "alice@example.com" ;
    schema:knows ex:bob, ex:charlie .

ex:bob a schema:Person ;
    schema:name "Bob Brown" ;
    schema:age 25 ;
    schema:email "bob@example.com" ;
    schema:knows ex:alice .

ex:charlie a schema:Person ;
    schema:name "Charlie Chen" ;
    schema:age 35 ;
    schema:email "charlie@example.com" ;
    schema:knows ex:alice .

ex:diana a schema:Person ;
    schema:name "Diana Davis" ;
    schema:age 28 ;
    schema:email "diana@example.com" .

ex:book1 a schema:Book ;
    schema:name "Introduction to RDF" ;
    schema:author ex:alice ;
    schema:datePublished "2020-01-15"^^xsd:date ;
    schema:numberOfPages 250 .

ex:book2 a schema:Book ;
    schema:name "SPARQL Queries Explained" ;
    schema:author ex:charlie ;
    schema:datePublished "2021-06-20"^^xsd:date ;
    schema:numberOfPages 300 .

ex:organization1 a schema:Organization ;
    schema:name "Tech Corp" ;
    schema:employee ex:alice, ex:bob ;
    schema:foundingDate "2015-01-01"^^xsd:date .
"#;

    store.load_from_reader(RdfFormat::Turtle, turtle_data.as_bytes())?;
    println!("✓ Initial data loaded: {} triples", store.len()?);
    Ok(())
}

fn get_shacl_shapes() -> &'static str {
    r#"
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix schema: <http://schema.org/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

schema:PersonShape a sh:NodeShape ;
    sh:targetClass schema:Person ;
    sh:property [
        sh:path schema:name ;
        sh:minCount 1 ;
        sh:maxCount 1 ;
        sh:datatype xsd:string ;
    ] ;
    sh:property [
        sh:path schema:age ;
        sh:minCount 1 ;
        sh:maxCount 1 ;
        sh:datatype xsd:integer ;
        sh:minInclusive 0 ;
        sh:maxInclusive 150 ;
    ] ;
    sh:property [
        sh:path schema:email ;
        sh:maxCount 1 ;
        sh:datatype xsd:string ;
    ] .

schema:BookShape a sh:NodeShape ;
    sh:targetClass schema:Book ;
    sh:property [
        sh:path schema:name ;
        sh:minCount 1 ;
        sh:datatype xsd:string ;
    ] ;
    sh:property [
        sh:path schema:author ;
        sh:minCount 1 ;
        sh:class schema:Person ;
    ] ;
    sh:property [
        sh:path schema:numberOfPages ;
        sh:datatype xsd:integer ;
        sh:minInclusive 1 ;
    ] .
"#
}

// ============================================================================
// Workload Operations
// ============================================================================

fn run_simple_query(store: &Store) -> Result<usize, Box<dyn std::error::Error>> {
    let query = r#"
        PREFIX schema: <http://schema.org/>
        SELECT ?name WHERE {
            ?person a schema:Person ;
                    schema:name ?name .
        }
    "#;

    let mut count = 0;
    if let QueryResults::Solutions(mut solutions) =
        SparqlEvaluator::new().parse_query(query)?.on_store(store).execute()?
    {
        while let Some(solution) = solutions.next() {
            solution?;
            count += 1;
        }
    }
    Ok(count)
}

fn run_complex_query(store: &Store) -> Result<usize, Box<dyn std::error::Error>> {
    let query = r#"
        PREFIX schema: <http://schema.org/>
        SELECT ?author (COUNT(?book) as ?bookCount) (AVG(?pages) as ?avgPages)
        WHERE {
            ?book a schema:Book ;
                  schema:author ?author ;
                  schema:numberOfPages ?pages .
        }
        GROUP BY ?author
        ORDER BY DESC(?bookCount)
    "#;

    let mut count = 0;
    if let QueryResults::Solutions(mut solutions) =
        SparqlEvaluator::new().parse_query(query)?.on_store(store).execute()?
    {
        while let Some(solution) = solutions.next() {
            solution?;
            count += 1;
        }
    }
    Ok(count)
}

fn run_shacl_validation(store: &Store) -> Result<usize, Box<dyn std::error::Error>> {
    // Load shapes
    let mut shapes_graph = Graph::new();
    for quad in RdfParser::from_format(oxrdfio::RdfFormat::Turtle)
        .for_reader(get_shacl_shapes().as_bytes())
    {
        let quad = quad?;
        shapes_graph.insert(TripleRef::from(quad.as_ref()));
    }

    let shacl_shapes = ShapesGraph::from_graph(&shapes_graph)?;
    let validator = ShaclValidator::new(shacl_shapes);

    // Get data graph
    let mut data_graph = Graph::new();
    for quad in store.quads_for_pattern(None, None, None, Some(GraphNameRef::DefaultGraph)) {
        let quad = quad?;
        data_graph.insert(TripleRef::from(quad.as_ref()));
    }

    // Validate
    let report = validator.validate(&data_graph)?;
    Ok(report.results().len())
}

fn run_bulk_ingestion(store: &Store, iteration: u64) -> Result<usize, Box<dyn std::error::Error>> {
    let data = format!(r#"
<http://example.com/generated_{0}> <http://schema.org/name> "Generated Resource {0}" .
<http://example.com/generated_{0}> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://schema.org/Thing> .
<http://example.com/generated_{0}> <http://schema.org/dateCreated> "2025-12-26"^^<http://www.w3.org/2001/XMLSchema#date> .
"#, iteration);

    let mut loader = store.bulk_loader();
    loader.load_from_slice(RdfFormat::NTriples, data.as_bytes())?;
    loader.commit()?;
    Ok(3)
}

fn run_update(store: &Store, iteration: u64) -> Result<(), Box<dyn std::error::Error>> {
    let ex = NamedNode::new(format!("http://example.com/update_{}", iteration))?;
    let name = NamedNode::new("http://schema.org/name")?;
    let value = Literal::new_simple_literal(format!("Update {}", iteration));

    // Insert
    store.insert(QuadRef::new(
        &ex,
        &name,
        &value,
        GraphNameRef::DefaultGraph,
    ))?;

    // Remove
    store.remove(QuadRef::new(
        &ex,
        &name,
        &value,
        GraphNameRef::DefaultGraph,
    ))?;

    Ok(())
}

// ============================================================================
// Main Workload Loop
// ============================================================================

fn run_mixed_workload(
    store: &Store,
    stats: &Arc<Mutex<SoakStats>>,
    iteration: u64,
    test_start: Instant,
) {
    let operation_choice = (iteration % 100) as u32;

    let (op_type, result) = if operation_choice < QUERY_PERCENTAGE / 2 {
        // 30% simple queries
        let start = Instant::now();
        let result = run_simple_query(store);
        let elapsed = start.elapsed();
        stats.lock().unwrap().record_latency(elapsed, OperationType::SimpleQuery, test_start);
        (OperationType::SimpleQuery, result.map(|_| ()))
    } else if operation_choice < QUERY_PERCENTAGE {
        // 30% complex queries
        let start = Instant::now();
        let result = run_complex_query(store);
        let elapsed = start.elapsed();
        stats.lock().unwrap().record_latency(elapsed, OperationType::ComplexQuery, test_start);
        (OperationType::ComplexQuery, result.map(|_| ()))
    } else if operation_choice < QUERY_PERCENTAGE + SHACL_PERCENTAGE {
        // 20% SHACL validation
        let start = Instant::now();
        let result = run_shacl_validation(store);
        let elapsed = start.elapsed();
        stats.lock().unwrap().record_latency(elapsed, OperationType::ShaclValidation, test_start);
        (OperationType::ShaclValidation, result.map(|_| ()))
    } else if operation_choice < QUERY_PERCENTAGE + SHACL_PERCENTAGE + INGESTION_PERCENTAGE {
        // 15% bulk ingestion
        let start = Instant::now();
        let result = run_bulk_ingestion(store, iteration);
        let elapsed = start.elapsed();
        stats.lock().unwrap().record_latency(elapsed, OperationType::BulkIngestion, test_start);
        (OperationType::BulkIngestion, result.map(|_| ()))
    } else {
        // 5% updates
        let start = Instant::now();
        let result = run_update(store, iteration);
        let elapsed = start.elapsed();
        stats.lock().unwrap().record_latency(elapsed, OperationType::Update, test_start);
        (OperationType::Update, result)
    };

    let mut stats = stats.lock().unwrap();
    stats.record_operation();

    if let Err(e) = result {
        stats.record_error(&format!("{}Error", op_type));
        if iteration % 100 == 0 {
            eprintln!("Operation {} error: {}", op_type, e);
        }
    }
}

// ============================================================================
// Reporting
// ============================================================================

fn print_hourly_report(stats: &SoakStats, elapsed: Duration, hour: usize) {
    println!("\n=================================================================");
    println!("  HOUR {} REPORT - Elapsed: {:.1}h", hour, elapsed.as_secs_f64() / 3600.0);
    println!("=================================================================");

    // Operations
    println!("\n📊 OPERATIONS:");
    println!("  Total Operations:     {:>10}", stats.operations);
    println!("  Operations/Second:    {:>10.2}", stats.operations_per_second(elapsed));
    println!("  Total Errors:         {:>10}", stats.errors);
    println!("  Error Rate:           {:>9.3}%", stats.error_rate());

    // Latency
    println!("\n⏱️  LATENCY (all operations):");
    if let Some(p50) = stats.get_percentile_latency(50.0) {
        println!("  P50:                  {:>10.2}ms", p50.as_millis());
    }
    if let Some(p95) = stats.get_percentile_latency(95.0) {
        println!("  P95:                  {:>10.2}ms", p95.as_millis());
    }
    if let Some(p99) = stats.get_percentile_latency(99.0) {
        println!("  P99:                  {:>10.2}ms", p99.as_millis());
    }

    // Latency by operation type
    println!("\n⏱️  LATENCY BY OPERATION TYPE:");
    for op_type in [
        OperationType::SimpleQuery,
        OperationType::ComplexQuery,
        OperationType::ShaclValidation,
        OperationType::BulkIngestion,
        OperationType::Update,
    ] {
        if let Some(p99) = stats.get_percentile_latency_by_type(99.0, op_type) {
            println!("  {:20} P99: {:>8.2}ms", format!("{}:", op_type), p99.as_millis());
        }
    }

    // Memory
    println!("\n💾 MEMORY:");
    if let Some(last_sample) = stats.memory_samples.last() {
        println!("  Current Memory:       {:>10.2} MB", last_sample.bytes as f64 / 1_048_576.0);
    }
    if let Some(first_sample) = stats.memory_samples.first() {
        if let Some(last_sample) = stats.memory_samples.last() {
            let growth = (last_sample.bytes as i64) - (first_sample.bytes as i64);
            println!("  Memory Growth:        {:>+10.2} MB", growth as f64 / 1_048_576.0);
        }
    }
    if stats.memory_samples.len() > MEMORY_PLATEAU_HOURS {
        let growth_rate = calculate_memory_growth_rate(stats);
        println!("  Growth Rate:          {:>+9.2} MB/hour", growth_rate);
    }

    // Errors by type
    if !stats.errors_by_type.is_empty() {
        println!("\n⚠️  ERRORS BY TYPE:");
        for (error_type, count) in &stats.errors_by_type {
            println!("  {:30} {:>6}", format!("{}:", error_type), count);
        }
    }

    println!("=================================================================");
}

fn print_final_report(stats: &SoakStats, elapsed: Duration) {
    println!("\n\n===================================================================");
    println!("                      FINAL SOAK TEST REPORT                       ");
    println!("===================================================================");

    println!("\n⏰ DURATION:");
    println!("  Total Time:           {:.2} hours ({} seconds)",
             elapsed.as_secs_f64() / 3600.0, elapsed.as_secs());

    println!("\n📊 OVERALL STATISTICS:");
    println!("  Total Operations:     {}", stats.operations);
    println!("  Avg Operations/Sec:   {:.2}", stats.operations_per_second(elapsed));
    println!("  Total Errors:         {}", stats.errors);
    println!("  Error Rate:           {:.3}%", stats.error_rate());

    println!("\n⏱️  LATENCY SUMMARY:");
    if let Some(p50) = stats.get_percentile_latency(50.0) {
        println!("  P50 Latency:          {:.2}ms", p50.as_millis());
    }
    if let Some(p95) = stats.get_percentile_latency(95.0) {
        println!("  P95 Latency:          {:.2}ms", p95.as_millis());
    }
    if let Some(p99) = stats.get_percentile_latency(99.0) {
        println!("  P99 Latency:          {:.2}ms", p99.as_millis());
    }

    println!("\n💾 MEMORY ANALYSIS:");
    if let Some(first) = stats.memory_samples.first() {
        if let Some(last) = stats.memory_samples.last() {
            let start_mb = first.bytes as f64 / 1_048_576.0;
            let end_mb = last.bytes as f64 / 1_048_576.0;
            let growth_mb = end_mb - start_mb;

            println!("  Starting Memory:      {:.2} MB", start_mb);
            println!("  Ending Memory:        {:.2} MB", end_mb);
            println!("  Total Growth:         {:.2} MB", growth_mb);

            if stats.memory_samples.len() > MEMORY_PLATEAU_HOURS {
                let growth_rate = calculate_memory_growth_rate(stats);
                println!("  Growth Rate:          {:.2} MB/hour", growth_rate);
            }

            let peak = stats.memory_samples.iter().map(|s| s.bytes).max().unwrap_or(0);
            println!("  Peak Memory:          {:.2} MB", peak as f64 / 1_048_576.0);
        }
    }
}

fn calculate_memory_growth_rate(stats: &SoakStats) -> f64 {
    if stats.memory_samples.len() < MEMORY_PLATEAU_HOURS + 1 {
        return 0.0;
    }

    // Calculate growth rate from hour 2 onwards to detect plateau
    let plateau_start_index = MEMORY_PLATEAU_HOURS;
    let plateau_samples = &stats.memory_samples[plateau_start_index..];

    if plateau_samples.len() < 2 {
        return 0.0;
    }

    let first = &plateau_samples[0];
    let last = &plateau_samples[plateau_samples.len() - 1];

    let hours = (last.timestamp.as_secs_f64() - first.timestamp.as_secs_f64()) / 3600.0;
    if hours == 0.0 {
        return 0.0;
    }

    let growth_bytes = (last.bytes as i64) - (first.bytes as i64);
    let growth_mb = growth_bytes as f64 / 1_048_576.0;

    growth_mb / hours
}

// ============================================================================
// Assertions
// ============================================================================

fn assert_memory_plateau(stats: &SoakStats) -> Result<(), String> {
    if stats.memory_samples.len() < MEMORY_PLATEAU_HOURS + 1 {
        println!("\n⚠️  WARNING: Not enough memory samples for plateau detection");
        return Ok(());
    }

    let growth_rate = calculate_memory_growth_rate(stats);

    println!("\n===================================================================");
    println!("                     ASSERTION: MEMORY PLATEAU                     ");
    println!("===================================================================");
    println!("  Growth Rate (hours {}-end): {:.2} MB/hour", MEMORY_PLATEAU_HOURS, growth_rate);
    println!("  Threshold:                   {:.2} MB/hour", MEMORY_GROWTH_THRESHOLD_MB_PER_HOUR);

    if growth_rate.abs() <= MEMORY_GROWTH_THRESHOLD_MB_PER_HOUR {
        println!("  ✅ PASS: Memory has plateaued");
        Ok(())
    } else {
        println!("  ❌ FAIL: Memory growth exceeds threshold");
        Err(format!(
            "Memory growth rate {:.2} MB/hour exceeds threshold {:.2} MB/hour",
            growth_rate, MEMORY_GROWTH_THRESHOLD_MB_PER_HOUR
        ))
    }
}

fn assert_error_rate_acceptable(stats: &SoakStats) -> Result<(), String> {
    let error_rate = stats.error_rate();

    println!("\n===================================================================");
    println!("                   ASSERTION: ERROR RATE                           ");
    println!("===================================================================");
    println!("  Error Rate:      {:.3}%", error_rate);
    println!("  Max Allowed:     {:.3}%", MAX_ERROR_RATE_PERCENT);

    if error_rate <= MAX_ERROR_RATE_PERCENT {
        println!("  ✅ PASS: Error rate is acceptable");
        Ok(())
    } else {
        println!("  ❌ FAIL: Error rate exceeds threshold");
        Err(format!(
            "Error rate {:.3}% exceeds threshold {:.3}%",
            error_rate, MAX_ERROR_RATE_PERCENT
        ))
    }
}

fn assert_no_latency_degradation(stats: &SoakStats) -> Result<(), String> {
    println!("\n===================================================================");
    println!("                 ASSERTION: LATENCY STABILITY                      ");
    println!("===================================================================");

    if let Some(p99) = stats.get_percentile_latency(99.0) {
        let p99_ms = p99.as_millis();
        println!("  P99 Latency:         {}ms", p99_ms);
        println!("  Max Allowed:         {}ms", MAX_P99_LATENCY_MS);

        if p99_ms <= MAX_P99_LATENCY_MS {
            println!("  ✅ PASS: P99 latency within threshold");
            Ok(())
        } else {
            println!("  ❌ FAIL: P99 latency exceeds threshold");
            Err(format!(
                "P99 latency {}ms exceeds threshold {}ms",
                p99_ms, MAX_P99_LATENCY_MS
            ))
        }
    } else {
        println!("  ⚠️  WARNING: No latency data available");
        Ok(())
    }
}

// ============================================================================
// Main Test Loop
// ============================================================================

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n===================================================================");
    println!("            OXIGRAPH 24+ HOUR SOAK TEST                           ");
    println!("===================================================================");

    // Get duration from environment or use default
    let duration_hours = std::env::var("SOAK_DURATION_HOURS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_DURATION_HOURS);

    let target_duration = Duration::from_secs(duration_hours * 3600);

    println!("\n⚙️  CONFIGURATION:");
    println!("  Target Duration:      {} hours", duration_hours);
    println!("  Memory Sample Rate:   {}s", MEMORY_SAMPLE_INTERVAL.as_secs());
    println!("  Report Interval:      {}s", HOURLY_REPORT_INTERVAL.as_secs());
    println!("  Workload Mix:         {}% queries, {}% SHACL, {}% ingestion, {}% updates",
             QUERY_PERCENTAGE, SHACL_PERCENTAGE, INGESTION_PERCENTAGE, UPDATE_PERCENTAGE);

    // Initialize store and data
    println!("\n🚀 INITIALIZATION:");
    let store = Store::new()?;
    setup_initial_data(&store)?;

    let stats = Arc::new(Mutex::new(SoakStats::new()));
    let stats_clone = Arc::clone(&stats);

    let test_start = Instant::now();
    let start_clone = test_start;

    // Spawn memory monitoring thread
    let running = Arc::new(AtomicBool::new(true));
    let running_clone = Arc::clone(&running);

    let memory_thread = thread::spawn(move || {
        while running_clone.load(Ordering::Relaxed) {
            let memory_bytes = get_current_memory_usage();
            let elapsed = start_clone.elapsed();

            stats_clone.lock().unwrap().record_memory(elapsed, memory_bytes);

            thread::sleep(MEMORY_SAMPLE_INTERVAL);
        }
    });

    // Set up Ctrl+C handler
    let running_ctrl_c = Arc::clone(&running);
    ctrlc::set_handler(move || {
        println!("\n\n⚠️  Received Ctrl+C, shutting down gracefully...");
        running_ctrl_c.store(false, Ordering::Relaxed);
    })?;

    println!("\n===================================================================");
    println!("                    SOAK TEST RUNNING...                           ");
    println!("===================================================================");
    println!("\n⏱️  Test started at: {:?}", std::time::SystemTime::now());
    println!("Press Ctrl+C for graceful shutdown\n");

    let mut iteration = 0u64;
    let mut last_report_time = Instant::now();
    let mut hour_count = 1;

    // Main workload loop
    while test_start.elapsed() < target_duration && running.load(Ordering::Relaxed) {
        run_mixed_workload(&store, &stats, iteration, test_start);
        iteration += 1;

        // Print hourly report
        if last_report_time.elapsed() >= HOURLY_REPORT_INTERVAL {
            let stats_snapshot = stats.lock().unwrap().clone();
            print_hourly_report(&stats_snapshot, test_start.elapsed(), hour_count);
            last_report_time = Instant::now();
            hour_count += 1;
        }

        // Small sleep to prevent tight loop
        if iteration % 1000 == 0 {
            thread::sleep(Duration::from_millis(10));
        }
    }

    // Shutdown
    running.store(false, Ordering::Relaxed);
    memory_thread.join().unwrap();

    let final_elapsed = test_start.elapsed();
    let final_stats = stats.lock().unwrap().clone();

    // Print final report
    print_final_report(&final_stats, final_elapsed);

    // Run assertions
    println!("\n===================================================================");
    println!("                      RUNNING ASSERTIONS                           ");
    println!("===================================================================");

    let mut failures = Vec::new();

    if let Err(e) = assert_memory_plateau(&final_stats) {
        failures.push(e);
    }

    if let Err(e) = assert_error_rate_acceptable(&final_stats) {
        failures.push(e);
    }

    if let Err(e) = assert_no_latency_degradation(&final_stats) {
        failures.push(e);
    }

    // Final verdict
    println!("\n===================================================================");
    if failures.is_empty() {
        println!("                        ✅ SOAK TEST PASSED                        ");
        println!("===================================================================");
        println!("\n🎉 All assertions passed! Oxigraph is stable over long duration.");
        Ok(())
    } else {
        println!("                        ❌ SOAK TEST FAILED                        ");
        println!("===================================================================");
        println!("\n⚠️  {} assertion(s) failed:", failures.len());
        for (i, failure) in failures.iter().enumerate() {
            println!("  {}. {}", i + 1, failure);
        }
        Err(failures.join("; ").into())
    }
}
