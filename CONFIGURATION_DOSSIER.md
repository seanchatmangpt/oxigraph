# RESOURCE LIMIT CONFIGURATION DOSSIER

**Status**: ⚠️ Gaps Identified - Production Deployment Requires Application-Level Safeguards

**Generated**: 2025-12-26
**Agent**: Agent 9 - Resource Limit Enforcement Test Builder
**Test Suite**: `/home/user/oxigraph/lib/oxigraph/tests/resource_limits.rs`

---

## Executive Summary

Oxigraph provides **foundational resource management** through streaming iterators and manual cancellation tokens, but **lacks automatic timeout and memory limit enforcement**. Production deployments MUST implement application-level safeguards to prevent denial-of-service vulnerabilities.

### Critical Findings

| Resource | Configurable | Default | Enforcement | Production Risk |
|----------|--------------|---------|-------------|-----------------|
| Query Timeout | ❌ No | None | Manual only | **HIGH** |
| Query Memory | ❌ No | Unbounded | None | **MEDIUM** |
| SHACL Timeout | ❌ No | None | None | **MEDIUM** |
| Result Streaming | ✅ Yes | Enabled | Automatic | LOW |
| SHACL Recursion | ⚠️ Fixed | 50 | Automatic | LOW |
| HTTP SERVICE | ✅ Yes | None | Configurable | LOW |

---

## QUERY EXECUTION

### Timeout

**Status**: ❌ **NOT AVAILABLE - Critical Gap**

**Configuration**: None

**Available Mechanism**: `CancellationToken` (requires manual implementation)

**Code Evidence**:
```rust
// lib/oxigraph/src/sparql/mod.rs:346
pub fn with_cancellation_token(mut self, cancellation_token: CancellationToken) -> Self {
    self.inner = self.inner.with_cancellation_token(cancellation_token);
    self
}
```

**Default Behavior**: Queries run **indefinitely** without manual cancellation.

**Production Mitigation** (REQUIRED):
```rust
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use oxigraph::sparql::{CancellationToken, SparqlEvaluator};
use oxigraph::store::Store;

fn run_query_with_timeout(
    store: &Store,
    query: &str,
    timeout: Duration,
) -> Result<QueryResults, Box<dyn std::error::Error>> {
    let cancellation_token = CancellationToken::new();
    let token_clone = cancellation_token.clone();

    // Application-level timeout
    thread::spawn(move || {
        thread::sleep(timeout);
        token_clone.cancel();
    });

    SparqlEvaluator::new()
        .with_cancellation_token(cancellation_token)
        .parse_query(query)?
        .on_store(store)
        .execute()
}
```

**Risk**: Without timeout wrapper, malicious queries can consume CPU indefinitely.

---

### Memory Limit

**Status**: ❌ **NOT CONFIGURABLE - Mitigated by Design**

**Configuration**: None

**Mitigation Strategy**: Results are **streamed via iterators** (not buffered).

**Code Evidence**:
```rust
// Results are iterators, not Vec<T>
pub enum QueryResults<'a> {
    Solutions(QuerySolutionIter<'a>),  // Iterator, not buffered
    Graph(QueryTripleIter<'a>),        // Iterator
    Boolean(bool),
}
```

**Default Behavior**: Queries stream results without buffering entire result set in memory.

**Production Safeguards**:
1. **Use LIMIT clauses** to bound result sets:
   ```sparql
   SELECT * WHERE { ?s ?p ?o } LIMIT 1000
   ```

2. **Container Memory Limits** (Kubernetes/Docker):
   ```yaml
   resources:
     limits:
       memory: "2Gi"
   ```

3. **Process-level limits** (Linux):
   ```bash
   ulimit -m 2097152  # 2GB memory limit
   ```

**Risk**: While results are streamed, complex queries can still consume excessive memory during evaluation (e.g., large JOIN operations).

---

### Result Set Limit

**Status**: ✅ **STREAMING - Application Controlled**

**Configuration**: Via SPARQL `LIMIT` clause

**Enforcement**: Results are **always streamed** as iterators

**Code Evidence**:
```rust
// lib/oxigraph/tests/resource_limits.rs
if let QueryResults::Solutions(mut solutions) = query_result {
    // Consume only needed results
    while let Some(Ok(solution)) = solutions.next() {
        // Process incrementally
    }
}
```

**Best Practice**:
- Always use `LIMIT` for unbounded queries
- Implement pagination for large result sets
- Set application-level maximum result count

**Risk**: LOW - Streaming design prevents memory exhaustion from large result sets.

---

## VALIDATION (SHACL)

### Recursion Depth Limit

**Status**: ✅ **ENFORCED - Not Configurable**

**Configuration**: Hardcoded to `50`

**Code Evidence**:
```rust
// lib/sparshacl/src/validator.rs:21
const MAX_RECURSION_DEPTH: usize = 50;

// lib/sparshacl/src/validator.rs:116
if depth > MAX_RECURSION_DEPTH {
    return Err(ShaclValidationError::max_recursion_depth(depth).into());
}
```

**Enforcement**: Automatic - validation fails with error if depth exceeds 50.

**Recommendation**: If 50 is insufficient, file feature request for configurable limit.

**Risk**: LOW - Prevents infinite recursion in shape validation.

---

### Timeout

**Status**: ❌ **NOT AVAILABLE - High Gap**

**Configuration**: None

**Code Evidence**:
```rust
// lib/sparshacl/src/validator.rs:42
pub fn validate(&self, data_graph: &Graph) -> Result<ValidationReport, ShaclError> {
    // No timeout parameter or mechanism
}
```

**Default Behavior**: Validation runs until completion or error.

**Production Mitigation**:
```rust
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

fn validate_with_timeout(
    validator: &ShaclValidator,
    data_graph: &Graph,
    timeout: Duration,
) -> Result<Option<ValidationReport>, Box<dyn std::error::Error>> {
    let (tx, rx) = mpsc::channel();
    let validator_clone = validator.clone();
    let graph_clone = data_graph.clone();

    thread::spawn(move || {
        let result = validator_clone.validate(&graph_clone);
        let _ = tx.send(result);
    });

    match rx.recv_timeout(timeout) {
        Ok(result) => Ok(Some(result?)),
        Err(mpsc::RecvTimeoutError::Timeout) => Ok(None),
        Err(mpsc::RecvTimeoutError::Disconnected) => Err("Validation thread died".into()),
    }
}
```

**Risk**: Complex shapes can cause validation to hang indefinitely.

---

### Memory Limit

**Status**: ❌ **NOT AVAILABLE**

**Configuration**: None

**Mitigation**: Container-level memory limits only

**Risk**: MEDIUM - Complex shapes with many constraints can consume significant memory.

---

## VALIDATION (ShEx)

**Status**: ⚠️ **Limited Implementation**

Oxigraph has basic ShEx support added recently. Resource limits follow similar pattern to SHACL:

- **Recursion Depth**: Likely enforced but not documented
- **Timeout**: Not available
- **Memory Limit**: Not configurable

**Recommendation**: Test ShEx validation and apply same mitigations as SHACL.

---

## REASONING (OWL)

**Status**: ⚠️ **Not Primary Feature**

Oxigraph is primarily an RDF store and does not include built-in OWL reasoning.

For OWL reasoning scenarios:
- **Iteration Limit**: N/A
- **Timeout**: N/A (would be application responsibility)
- **Memory Limit**: N/A

**Note**: If using Oxigraph with external reasoners, implement timeouts in the integration layer.

---

## INGESTION

### Bulk Loader Configuration

**Status**: ⚠️ **Partial - Missing Timeout**

**Available Configuration**:
```rust
// lib/oxigraph/src/store.rs:1295
pub fn bulk_loader(&self) -> BulkLoader<'_> {
    BulkLoader {
        storage: self.storage.bulk_loader(),
        num_threads: None,       // ✅ Configurable
        max_memory_size: None,   // ✅ Configurable
        on_parse_error: None,
    }
}
```

**Code Evidence**:
```rust
let loader = store.bulk_loader()
    .with_num_threads(4)           // Control parallelism
    .with_max_memory_size(1_000);  // Limit memory usage
```

**Missing**: Timeout configuration

**Production Mitigation**:
```rust
use std::thread;
use std::time::Duration;

fn bulk_load_with_timeout(
    store: &Store,
    data: impl BufRead,
    format: RdfFormat,
    timeout: Duration,
) -> Result<(), Box<dyn std::error::Error>> {
    let store_clone = store.clone();
    let handle = thread::spawn(move || {
        store_clone.bulk_loader()
            .with_num_threads(4)
            .load_from_read(format, data)
    });

    match handle.join_timeout(timeout) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(e.into()),
        Err(_) => Err("Bulk load timeout".into()),
    }
}
```

**Risk**: MEDIUM - Large file ingestion without timeout can block indefinitely.

---

### Buffer Size

**Status**: ⚠️ **Internal - Not Configurable**

**Observation**: Buffer sizes are implementation details in RocksDB wrapper.

**Recommendation**: Use `max_memory_size` parameter on BulkLoader to indirectly control buffering.

---

## HTTP SERVICE (Federated Queries)

### Timeout

**Status**: ✅ **CONFIGURABLE - Complete**

**Configuration**:
```rust
// lib/oxigraph/src/sparql/mod.rs:196
#[cfg(feature = "http-client")]
pub fn with_http_timeout(mut self, timeout: Duration) -> Self {
    self.http_timeout = Some(timeout);
    self
}
```

**Usage**:
```rust
let evaluator = SparqlEvaluator::new()
    .with_http_timeout(Duration::from_secs(30))
    .with_http_redirection_limit(5);
```

**Default**: None (no timeout)

**Production Recommendation**: Always set HTTP timeout for federated queries.

**Risk**: LOW - Configurable timeout prevents hanging on slow/unresponsive endpoints.

---

### Redirection Limit

**Status**: ✅ **CONFIGURABLE**

**Default**: `0` (no redirects allowed)

**Code Evidence**:
```rust
// lib/oxigraph/src/sparql/mod.rs:206
pub fn with_http_redirection_limit(mut self, redirection_limit: usize) -> Self {
    self.http_redirection_limit = redirection_limit;
    self
}
```

**Recommendation**: Set to reasonable value (3-5) if redirects are expected.

---

## GAPS & RECOMMENDATIONS

### Critical Gaps (MUST Fix for Production)

1. **No Default Query Timeout**
   - **Impact**: Queries can run indefinitely
   - **Mitigation**: Implement application-level timeout wrapper (see code example above)
   - **Urgency**: **CRITICAL**

2. **No SHACL Validation Timeout**
   - **Impact**: Complex shape validation can hang
   - **Mitigation**: Wrap validation in thread with timeout
   - **Urgency**: **HIGH**

3. **No Bulk Loader Timeout**
   - **Impact**: Large file ingestion can block indefinitely
   - **Mitigation**: Spawn loader in thread with timeout
   - **Urgency**: **MEDIUM**

---

### High Gaps (SHOULD Address)

4. **No Query Memory Limits**
   - **Impact**: Complex queries can exhaust memory
   - **Mitigation**: Container limits + LIMIT clauses + monitoring
   - **Urgency**: **MEDIUM**

5. **SHACL Recursion Depth Not Configurable**
   - **Impact**: Fixed limit of 50 may be insufficient
   - **Mitigation**: File feature request if needed
   - **Urgency**: **LOW**

---

### Low Gaps (NICE to Have)

6. **No RocksDB Tuning Exposure**
   - **Impact**: Cannot fine-tune storage performance
   - **Mitigation**: Accept defaults or build with custom RocksDB config
   - **Urgency**: **LOW**

---

## FOR PRODUCTION DEPLOYMENT

### Required Safeguards

- [x] **Test suite created**: `/home/user/oxigraph/lib/oxigraph/tests/resource_limits.rs`
- [ ] **Set query timeout wrapper**: Implement CancellationToken pattern
- [ ] **Configure container memory limit**: Set in Kubernetes/Docker
- [ ] **Use LIMIT clauses**: Bound all unbounded queries
- [ ] **Set HTTP SERVICE timeout**: 30s recommended
- [ ] **Monitor resource usage**: Prometheus/Grafana metrics
- [ ] **Implement application-level rate limiting**: Protect SPARQL endpoint

---

### Deployment Checklist

```yaml
# Example Kubernetes deployment
apiVersion: v1
kind: Deployment
metadata:
  name: oxigraph
spec:
  template:
    spec:
      containers:
      - name: oxigraph
        image: oxigraph/oxigraph:latest
        resources:
          limits:
            memory: "4Gi"      # ✅ Container limit
            cpu: "2"
          requests:
            memory: "2Gi"
            cpu: "1"
        env:
        - name: QUERY_TIMEOUT_SECONDS
          value: "30"          # ✅ Application implements timeout
        readinessProbe:
          httpGet:
            path: /
            port: 7878
          initialDelaySeconds: 10
          timeoutSeconds: 5    # ✅ Health check timeout
```

---

### Application-Level Wrapper

**Recommended**: Create `SafeOxigraph` wrapper that enforces timeouts:

```rust
pub struct SafeOxigraph {
    store: Store,
    default_timeout: Duration,
}

impl SafeOxigraph {
    pub fn query(&self, sparql: &str) -> Result<QueryResults, Error> {
        self.query_with_timeout(sparql, self.default_timeout)
    }

    pub fn query_with_timeout(
        &self,
        sparql: &str,
        timeout: Duration,
    ) -> Result<QueryResults, Error> {
        let token = CancellationToken::new();
        let token_clone = token.clone();

        thread::spawn(move || {
            thread::sleep(timeout);
            token_clone.cancel();
        });

        SparqlEvaluator::new()
            .with_cancellation_token(token)
            .parse_query(sparql)?
            .on_store(&self.store)
            .execute()
    }
}
```

---

## MONITORING REQUIREMENTS

### Essential Metrics

1. **Query Execution Time** (histogram)
   - Alert if p95 > 5s
   - Alert if p99 > 30s

2. **Query Cancellations** (counter)
   - Track timeout frequency
   - Investigate patterns

3. **Memory Usage** (gauge)
   - Alert if > 80% container limit
   - Track per-query memory if possible

4. **Active Query Count** (gauge)
   - Alert if > concurrency limit
   - Implement queue if needed

5. **Validation Time** (histogram)
   - Track SHACL validation duration
   - Alert if > 10s

---

## TESTING VERIFICATION

Run the resource limits test suite:

```bash
cd /home/user/oxigraph
cargo test -p oxigraph --test resource_limits
```

Expected output:
- ✅ All tests pass
- ⚠️  Tests document gaps in output
- 📋 Summary printed at end

---

## CONCLUSION

**Oxigraph Production Readiness**: ⚠️ **CONDITIONAL**

Oxigraph is production-ready **with proper application-level safeguards**:

1. ✅ **Streaming architecture** prevents result set memory exhaustion
2. ✅ **CancellationToken** provides timeout mechanism (manual)
3. ✅ **HTTP timeouts** configurable for federated queries
4. ❌ **No automatic timeouts** - requires application wrapper
5. ❌ **No memory limits** - requires container limits

**Bottom Line**: Oxigraph provides the **building blocks** for resource management, but production deployments **MUST** implement application-level enforcement of timeouts and monitoring.

---

**Sign-off**: Agent 9
**Date**: 2025-12-26
**Status**: Configuration documented, gaps identified, mitigations provided
