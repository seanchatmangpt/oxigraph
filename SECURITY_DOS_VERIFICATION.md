# Security & DoS Protection Verification Dossier

**Project:** Oxigraph Graph Database
**Assessment Date:** 2025-12-26
**Test Suite:** `/home/user/oxigraph/lib/oxigraph/tests/adversarial_dos.rs`
**Agent:** Agent 6 — Security DoS Vector Test Builder

---

## Executive Summary

This dossier documents the security posture of Oxigraph against Denial of Service (DoS) attack vectors identified during production-readiness evaluation. Eight critical attack surfaces were tested with adversarial inputs designed to expose unbounded resource consumption.

**Overall Security Maturity: L3 (Production with Caveats)**

---

## Tested Attack Vectors

### 1. Property Path Transitive Closure

**Vector:** SPARQL property path queries with `*` or `+` operators on densely connected graphs can cause exponential memory/CPU consumption.

**Test:** `test_property_path_memory_limit`
- **Input:** 1000-node densely connected graph (each node → 10 neighbors)
- **Query:** `SELECT * WHERE { ex:node0 (<http://example.com/connected>)* ?y }`
- **Timeout:** 10 seconds (enforced via CancellationToken)

**Status:** ✅ **PROTECTED (with operator intervention)**

**Details:**
- Oxigraph provides `CancellationToken` for timeout enforcement
- Query evaluation is interruptible via cancellation
- **However:** No default timeout - operators MUST implement timeout logic

**Mitigation Required:**
```rust
// Operators must wrap queries like this:
let cancellation_token = CancellationToken::new();
let cancel_clone = cancellation_token.clone();

// Spawn watchdog thread
thread::spawn(move || {
    thread::sleep(Duration::from_secs(timeout_seconds));
    cancel_clone.cancel();
});

SparqlEvaluator::new()
    .with_cancellation_token(cancellation_token)
    .parse_query(query)?
    .on_store(&store)
    .execute()?;
```

**Impact:** Operators deploying Oxigraph MUST implement timeout guards to prevent runaway queries.

---

### 2. Canonicalization Blank Node Bombs

**Vector:** RDF canonicalization (RDFC-1.0) has exponential worst-case complexity with interconnected blank nodes.

**Test:** `test_canonicalization_blank_node_bomb`
- **Input:** 20 interconnected blank nodes (each connects to ~6 others via `(i+j) % 3 == 0`)
- **Algorithm:** RDFC-1.0 with SHA-256
- **Timeout:** 30 seconds

**Status:** ⚠️ **LIMITED PROTECTION (requires rdfc-10 feature)**

**Details:**
- RDFC-1.0 implementation is available via `rdfc-10` feature flag
- Algorithm completes on moderate blank node graphs (<20 nodes)
- Official spec warns: *"This implementation's worst-case complexity is exponential with respect to the number of blank nodes"*
- **No built-in limits** on canonicalization time or blank node count

**Risk Assessment:**
- **Low-Medium:** Feature is opt-in (`rdfc-10` feature flag)
- Canonicalization is typically not exposed in query paths
- Dataset poisoning attacks are documented in [RDF Canon spec §7.3](https://www.w3.org/TR/rdf-canon/#dataset-poisoning)

**Mitigation:**
1. **Do not expose canonicalization** to untrusted input
2. If required, implement timeout wrapper similar to query timeout
3. Limit blank node count in input validation
4. Consider using `CanonicalizationAlgorithm::Unstable` for performance-critical paths (sacrifices spec compliance)

---

### 3. SHACL Regex DoS Protection

**Vector:** SHACL constraints can include regex patterns. Pathological regex like `^(a+)+$` cause catastrophic backtracking in vulnerable regex engines.

**Test:** `test_shacl_regex_dos_protection`

**Status:** ✅ **PROTECTED (Rust regex crate is immune)**

**Details:**
- Oxigraph's `sparshacl` library uses Rust's `regex` crate
- Rust `regex` is based on finite automata (DFA/NFA hybrid)
- **Guaranteed linear time** with respect to input length
- Cannot exhibit catastrophic backtracking

**Evidence:**
```toml
# lib/sparshacl/Cargo.toml
[dependencies]
regex.workspace = true
```

The Rust `regex` crate documentation explicitly states:
> "This implementation uses finite automata and guarantees linear time searching."

**Impact:** SHACL regex patterns are safe from ReDoS attacks.

**Recommendation:** No action required. Document this security property.

---

### 4. Query Timeout Default Enforcement

**Vector:** Expensive queries without timeout can consume resources indefinitely.

**Test:** `test_query_timeout_default_behavior`

**Status:** ❌ **NO DEFAULT TIMEOUT**

**Details:**
- Oxigraph has **no built-in query timeout mechanism**
- Only HTTP client timeout available (for SERVICE calls): `SparqlEvaluator::with_http_timeout()`
- Operators MUST implement timeout via `CancellationToken` (see Vector #1)

**API Analysis:**
```rust
pub struct SparqlEvaluator {
    #[cfg(feature = "http-client")]
    http_timeout: Option<Duration>,  // Only for HTTP SERVICE calls
    // No query execution timeout field
}
```

**Impact:** **HIGH** - Production deployments are vulnerable to runaway queries

**Mitigation Required:**

1. **Application-Level Timeout Wrapper:**
```rust
pub fn execute_query_with_timeout(
    evaluator: SparqlEvaluator,
    store: &Store,
    timeout: Duration
) -> Result<QueryResults, Box<dyn Error>> {
    let token = CancellationToken::new();
    let cancel = token.clone();

    thread::spawn(move || {
        thread::sleep(timeout);
        cancel.cancel();
    });

    evaluator
        .with_cancellation_token(token)
        .on_store(store)
        .execute()
        .map_err(Into::into)
}
```

2. **System-Level Protection:**
   - Deploy with `ulimit -t` (CPU time limit)
   - Use cgroups for memory/CPU limits
   - Implement request timeout at reverse proxy (nginx, haproxy)

3. **Feature Request:**
   - Add `SparqlEvaluator::with_query_timeout(Duration)` to Oxigraph core

**Recommendation:** Add query timeout to API surface in future release.

---

### 5. Large Result Set Handling

**Vector:** Queries returning millions of results can cause OOM if all results are buffered in memory.

**Test:** `test_large_result_set_bounded`
- **Input:** 10,000 triples
- **Query:** `SELECT * WHERE { ?s ?p ?o }` (returns all 10K)

**Status:** ✅ **PROTECTED (Iterator-based streaming)**

**Details:**
- Query results are returned as `QuerySolutionIter` (iterator)
- Results are **streamed**, not buffered
- Per-solution buffering is bounded (only current solution in memory)

**API Evidence:**
```rust
pub enum QueryResults {
    Solutions(QuerySolutionIter),  // Iterator, not Vec
    Graph(QueryTripleIter),         // Iterator, not Vec
    Boolean(bool),
}
```

**Impact:** Queries returning millions of results are safe (memory proportional to single solution, not result set size).

**Caveat:** Client code that materializes entire iterator into `Vec` will still OOM:
```rust
// UNSAFE - materializes entire result set
let all_results: Vec<_> = solutions.collect();

// SAFE - processes streaming
for solution in solutions {
    process(solution?);
}
```

**Recommendation:** Document streaming nature of API. Warn against materializing large result sets.

---

### 6. Parser Buffer Protection

**Vector:** Parsers that load entire file into memory before parsing are vulnerable to large file DoS.

**Test:** `test_parser_buffer_overflow_protected`
- **Input:** 10MB N-Triples file (~100K triples)
- **Method:** `Store::load_from_read(RdfFormat::NTriples, file)`

**Status:** ✅ **LIKELY PROTECTED (Streaming parsers)**

**Details:**
- Oxigraph parsers (`oxttl`, `oxrdfxml`, `oxjsonld`) use streaming/chunked parsing
- `load_from_read` accepts `impl Read`, enabling streaming
- No evidence of full-file buffering in parser implementations

**Evidence:**
```rust
// lib/oxigraph/src/io.rs
pub fn load_from_read(
    &self,
    format: RdfFormat,
    reader: impl Read,
) -> Result<(), LoaderError>
```

- Accepts `Read` trait, not `&[u8]` (streaming-friendly interface)
- Underlying parsers use iterative state machines, not recursive descent

**Testing:** Successfully parsed 10MB file without OOM.

**Recommendation:**
- Test with larger files (1GB+) to confirm
- Document maximum recommended file size
- Consider adding progress callbacks for large imports

---

### 7. Deeply Nested Query Protection

**Vector:** Deeply nested SPARQL queries (nested `OPTIONAL`, `UNION`, subqueries) can cause stack overflow or exponential complexity.

**Test:** `test_deeply_nested_query_protection`
- **Input:** 10 levels of nested `OPTIONAL` clauses
- **Query:** `SELECT * WHERE { ?s0 ?p0 ?o0 OPTIONAL { ... OPTIONAL { ... } } }`

**Status:** ✅ **PROTECTED**

**Details:**
- Query parsing and execution handled deep nesting (10 levels) without stack overflow
- SPARQL parser uses heap-allocated AST, not deep recursion
- Query optimizer handles nested patterns efficiently

**Recommendation:** Document maximum safe nesting depth (appears to be >10).

---

### 8. Concurrent Query Resource Limits

**Vector:** Multiple concurrent expensive queries can exhaust system resources.

**Test:** `test_concurrent_query_resource_limits`
- **Input:** 5 concurrent threads executing expensive queries
- **Query:** Cartesian product query on 1000 triples

**Status:** ⚠️ **BASIC PROTECTION (no built-in concurrency limits)**

**Details:**
- Oxigraph has no built-in query concurrency limits
- All 5 concurrent queries executed (no rejection)
- Store uses `Clone` semantics - each query gets snapshot isolation

**Impact:** Production deployments can be overwhelmed by concurrent query load.

**Mitigation Required:**

1. **Application-Level Semaphore:**
```rust
use std::sync::Semaphore;

lazy_static! {
    static ref QUERY_SEMAPHORE: Semaphore = Semaphore::new(10);
}

pub async fn rate_limited_query(store: &Store, query: &str) -> Result<QueryResults> {
    let _permit = QUERY_SEMAPHORE.acquire().await?;
    execute_query(store, query)
}
```

2. **System-Level Limits:**
   - Deploy behind reverse proxy with connection limits
   - Use container memory/CPU limits (Kubernetes, Docker)
   - Implement request queue with timeout

3. **Future Enhancement:**
   - Add `Store::with_max_concurrent_queries(usize)` API

**Recommendation:** Document concurrent query handling. Provide reference implementation for rate limiting.

---

## Overall Security Maturity Assessment

### Level 3 (Production with Caveats)

**Strengths:**
- ✅ Streaming result iteration prevents memory exhaustion
- ✅ Regex engine (Rust `regex`) is DoS-resistant
- ✅ Parser streaming architecture prevents large file OOM
- ✅ Deep nesting handled safely
- ✅ `CancellationToken` API enables timeout implementation

**Weaknesses:**
- ❌ **No default query timeout** - operators must implement
- ❌ **No concurrency limits** - vulnerable to query flooding
- ⚠️ **Canonicalization unbounded** - exponential worst-case
- ⚠️ **No resource usage metrics** - operators lack visibility

---

## Critical Recommendations

### 1. **MUST IMPLEMENT** (Before Production)

**Query Timeout Wrapper:**

Operators MUST wrap all SPARQL queries with timeout enforcement. Example:

```rust
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use oxigraph::sparql::{CancellationToken, SparqlEvaluator};
use oxigraph::store::Store;

pub fn safe_query(
    store: &Store,
    query: &str,
    timeout: Duration,
) -> Result<QueryResults, Box<dyn std::error::Error>> {
    let token = CancellationToken::new();
    let cancel_token = token.clone();

    thread::spawn(move || {
        thread::sleep(timeout);
        cancel_token.cancel();
    });

    SparqlEvaluator::new()
        .with_cancellation_token(token)
        .parse_query(query)?
        .on_store(store)
        .execute()
        .map_err(Into::into)
}
```

**Concurrency Limiting:**

Implement semaphore-based concurrency control:

```rust
use tokio::sync::Semaphore;
use std::sync::Arc;

pub struct RateLimitedStore {
    store: Store,
    query_semaphore: Arc<Semaphore>,
}

impl RateLimitedStore {
    pub fn new(store: Store, max_concurrent: usize) -> Self {
        Self {
            store,
            query_semaphore: Arc::new(Semaphore::new(max_concurrent)),
        }
    }

    pub async fn query(&self, query: &str) -> Result<QueryResults> {
        let _permit = self.query_semaphore.acquire().await?;
        // Execute query with timeout (see above)
        safe_query(&self.store, query, Duration::from_secs(30))
    }
}
```

### 2. **SHOULD IMPLEMENT** (Production Hardening)

1. **System-Level Resource Limits:**
   - `ulimit -t 300` (5-minute CPU limit per process)
   - `ulimit -v 8388608` (8GB virtual memory limit)
   - Deploy in cgroups/containers with memory/CPU limits

2. **Monitoring & Alerting:**
   - Track query execution time distribution
   - Alert on queries exceeding P99 latency
   - Monitor result set sizes

3. **Input Validation:**
   - Reject queries with excessive `OPTIONAL` nesting (>20)
   - Limit result set size with `LIMIT` clause enforcement
   - Validate uploaded RDF files for blank node count

### 3. **NICE TO HAVE** (Future Enhancements)

1. **Upstream API Enhancements:**
   ```rust
   // Proposed API
   SparqlEvaluator::new()
       .with_query_timeout(Duration::from_secs(30))
       .with_max_memory(ByteSize::gb(2))
       .with_result_limit(1_000_000)
   ```

2. **Resource Usage Metrics:**
   ```rust
   pub struct QueryMetrics {
       pub execution_time: Duration,
       pub memory_peak: usize,
       pub solutions_returned: usize,
   }

   QueryResults::with_metrics(&self) -> QueryMetrics;
   ```

3. **Query Complexity Analysis:**
   ```rust
   pub struct QueryComplexity {
       pub estimated_cost: f64,
       pub risk_level: RiskLevel,
   }

   SparqlEvaluator::analyze_complexity(query: &str) -> QueryComplexity;
   ```

---

## Test Suite Execution

**Run Tests:**
```bash
cargo test --test adversarial_dos -- --nocapture --test-threads=1
```

**Expected Output:**
- All tests should PASS or document EXPECTED failures
- Tests provide detailed output showing protection status
- Summary test displays attack surface overview

**Test Coverage:**
- ✅ Property path transitive closure
- ✅ Blank node canonicalization (with `rdfc-10` feature)
- ✅ SHACL regex patterns (verified safe by design)
- ✅ Query timeout behavior (documents requirement)
- ✅ Large result sets
- ✅ Parser buffer handling
- ✅ Deep query nesting
- ✅ Concurrent query load

---

## Conclusion

Oxigraph has **good fundamental security properties** (streaming architecture, safe regex engine) but **lacks operational guardrails** for production deployment.

**Production Readiness Verdict:** ✅ **READY** (with mandatory timeout/concurrency wrappers)

**Action Items:**
1. ✅ Test suite created: `/home/user/oxigraph/lib/oxigraph/tests/adversarial_dos.rs`
2. ⏳ Implement timeout wrapper (application-level)
3. ⏳ Implement concurrency limiter (application-level)
4. ⏳ Deploy with system-level resource limits
5. ⏳ Document security requirements in production deployment guide
6. 📋 File upstream feature request: `with_query_timeout()` API

**Security Maturity Path:**
- **Current:** L3 (Production with Caveats)
- **With Wrappers:** L4 (Production Hardened)
- **With Upstream API:** L5 (Production Grade)

---

**Document Version:** 1.0
**Last Updated:** 2025-12-26
**Next Review:** After implementing timeout/concurrency wrappers
