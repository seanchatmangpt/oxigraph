# OXIGRAPH PRODUCTION CONFIGURATION CHECKLIST

**Version**: 1.0
**Date**: 2025-12-26
**Target**: Production deployment of Oxigraph SPARQL + SHACL stack

---

## Overview

This checklist covers ALL required configurations for production deployment of Oxigraph. It distinguishes between:
- **MANDATORY** configurations (must set, no safe defaults)
- **AUTOMATIC** configurations (handled by system)
- **OPTIONAL** configurations (tuning and optimization)
- **EXTERNAL** configurations (infrastructure, not Oxigraph-specific)

---

## 🔴 MANDATORY CONFIGURATIONS (Must Set)

These configurations MUST be set before production deployment. Failure to configure these creates security vulnerabilities or operational risks.

### 1. Query Timeout ⏱️

**Requirement**: CRITICAL
**Default**: None (unlimited) ❌
**Risk**: DoS vulnerability, resource exhaustion
**Must Set**: YES

**Configuration**:

```rust
// Rust
use std::time::Duration;
let mut store = Store::open("./data")?;
store.set_query_timeout(Duration::from_secs(30));
```

```javascript
// JavaScript/WASM
const store = new Store();
store.queryTimeout = 30000; // milliseconds
```

```python
# Python
store = Store("./data")
store.query_timeout = 30  # seconds
```

**CLI Server**:
```bash
# Via environment variable (if supported)
OXIGRAPH_QUERY_TIMEOUT=30 oxigraph serve --location ./data

# Or via application wrapper
# (No direct CLI flag exists, requires code modification)
```

**Recommended Values**:
- **Interactive queries**: 10-30 seconds
- **Batch/analytics**: 300 seconds (5 minutes)
- **Public endpoints**: 10 seconds (strict)

**Verification**:
```bash
# Test timeout enforcement
curl -X POST http://localhost:7878/query \
  -H "Content-Type: application/sparql-query" \
  -d "SELECT * WHERE { ?s ?p ?o } ORDER BY ?s"  # Should timeout if large
```

---

### 2. Container Memory Limits 💾

**Requirement**: CRITICAL
**Default**: Unlimited (Docker) ❌
**Risk**: OOM crashes, node instability
**Must Set**: YES

**Configuration**:

```bash
# Docker
docker run -d \
  --name oxigraph \
  --memory=8g \
  --memory-swap=8g \
  --oom-kill-disable=false \
  oxigraph-server

# Kubernetes
apiVersion: v1
kind: Pod
metadata:
  name: oxigraph
spec:
  containers:
  - name: oxigraph
    image: oxigraph-server:latest
    resources:
      requests:
        memory: "4Gi"
      limits:
        memory: "8Gi"

# Systemd
# /etc/systemd/system/oxigraph.service
[Service]
MemoryLimit=8G
MemoryMax=8G
```

**Recommended Values**:
- **Small datasets** (<1M triples): 2-4GB
- **Medium datasets** (1M-10M triples): 4-8GB
- **Large datasets** (>10M triples): 8-16GB
- **Add headroom**: 2x expected working set size

**Calculation**:
```
Memory = Store size + Query working set + OS buffer
        = (RocksDB cache + memtables) + (result sets + indexes) + (1-2GB)

Example for 10M triples:
  Store size: ~2GB (RocksDB data + indexes)
  Query working set: ~2GB (result sets, sorting)
  OS buffer: ~2GB
  Total: 6GB → Allocate 8GB
```

**Verification**:
```bash
# Check container memory
docker stats oxigraph

# Monitor memory usage
curl http://localhost:7878/metrics | grep memory  # (if metrics endpoint exists)
```

---

### 3. Monitoring & Alerting 📊

**Requirement**: CRITICAL
**Default**: None ❌
**Risk**: Blind to issues, delayed incident response
**Must Set**: YES

**Minimum Monitoring** (MUST HAVE):

| Metric | Alert Threshold | Purpose |
|--------|----------------|---------|
| Query latency (p99) | > 1 second | Detect performance degradation |
| Query error rate | > 1% | Detect failures |
| Memory usage | > 80% of limit | Prevent OOM |
| Disk usage | > 80% of capacity | Prevent disk full |
| Store size | Unexpected growth | Detect data issues |

**Implementation**:

```rust
// Custom metrics wrapper (required until built-in metrics exist)
use prometheus::{Registry, Counter, Histogram, Gauge};

struct MonitoredStore {
    store: Store,
    query_duration: Histogram,
    query_count: Counter,
    query_errors: Counter,
    store_size: Gauge,
}

impl MonitoredStore {
    fn query(&self, query: &str) -> Result<QueryResults> {
        let start = Instant::now();
        let result = self.store.query(query);
        let duration = start.elapsed();

        self.query_duration.observe(duration.as_secs_f64());
        self.query_count.inc();

        if result.is_err() {
            self.query_errors.inc();
        }

        result
    }

    fn metrics_endpoint(&self) -> String {
        // Expose Prometheus metrics on /metrics
        prometheus::TextEncoder::new().encode_to_string(&self.registry).unwrap()
    }
}
```

**Prometheus Configuration**:
```yaml
# prometheus.yml
scrape_configs:
  - job_name: 'oxigraph'
    static_configs:
      - targets: ['oxigraph:7878']
    metrics_path: '/metrics'
    scrape_interval: 15s
```

**Grafana Dashboard** (minimum panels):
- Query latency (p50, p95, p99) - Line chart
- Query throughput (QPS) - Line chart
- Error rate (%) - Line chart
- Memory usage (%) - Gauge
- Store size (triples) - Line chart
- Disk usage (GB) - Line chart

**Alert Rules** (Prometheus):
```yaml
groups:
  - name: oxigraph
    rules:
      - alert: HighQueryLatency
        expr: histogram_quantile(0.99, query_duration_seconds) > 1
        for: 5m
        annotations:
          summary: "Query latency p99 > 1s"

      - alert: HighErrorRate
        expr: rate(query_errors_total[5m]) / rate(query_count_total[5m]) > 0.01
        for: 5m
        annotations:
          summary: "Query error rate > 1%"

      - alert: HighMemoryUsage
        expr: memory_usage_bytes / memory_limit_bytes > 0.8
        for: 10m
        annotations:
          summary: "Memory usage > 80%"
```

**Verification**:
```bash
# Check metrics endpoint
curl http://localhost:7878/metrics

# Test alerting
# (Trigger high load, verify alerts fire)
```

---

### 4. Backup Strategy 💾

**Requirement**: CRITICAL
**Default**: None ❌
**Risk**: Data loss, no disaster recovery
**Must Set**: YES

**Configuration**:

```bash
# Daily RocksDB snapshots
#!/bin/bash
# /usr/local/bin/backup-oxigraph.sh

DATE=$(date +%Y%m%d)
BACKUP_DIR="/backups/oxigraph"
DATA_DIR="/data/oxigraph"

# Create RocksDB backup
rsync -av --delete "$DATA_DIR" "$BACKUP_DIR/oxigraph-$DATE"

# Compress backup
tar -czf "$BACKUP_DIR/oxigraph-$DATE.tar.gz" "$BACKUP_DIR/oxigraph-$DATE"
rm -rf "$BACKUP_DIR/oxigraph-$DATE"

# Retain last 30 days
find "$BACKUP_DIR" -name "oxigraph-*.tar.gz" -mtime +30 -delete

# Upload to S3 (optional)
aws s3 cp "$BACKUP_DIR/oxigraph-$DATE.tar.gz" s3://backups/oxigraph/
```

**Cron Schedule**:
```cron
# Daily backup at 2 AM
0 2 * * * /usr/local/bin/backup-oxigraph.sh
```

**Kubernetes CronJob**:
```yaml
apiVersion: batch/v1
kind: CronJob
metadata:
  name: oxigraph-backup
spec:
  schedule: "0 2 * * *"
  jobTemplate:
    spec:
      template:
        spec:
          containers:
          - name: backup
            image: backup-tools:latest
            command: ["/scripts/backup-oxigraph.sh"]
            volumeMounts:
            - name: data
              mountPath: /data
            - name: backups
              mountPath: /backups
          restartPolicy: OnFailure
```

**Recovery Testing** (MANDATORY):
```bash
# Test restore procedure (quarterly)
# 1. Stop Oxigraph
docker stop oxigraph

# 2. Restore from backup
tar -xzf /backups/oxigraph-20251226.tar.gz -C /data

# 3. Start Oxigraph
docker start oxigraph

# 4. Verify data integrity
curl -X POST http://localhost:7878/query \
  -d "query=SELECT (COUNT(*) AS ?count) WHERE { ?s ?p ?o }"

# Expected: Count matches pre-backup count
```

**Retention Policy**:
- Daily backups: 30 days
- Weekly backups: 90 days
- Monthly backups: 1 year

**Verification**:
```bash
# Check backup exists
ls -lh /backups/oxigraph-*.tar.gz

# Check backup integrity
tar -tzf /backups/oxigraph-latest.tar.gz > /dev/null
```

---

### 5. Access Control & Authentication 🔐

**Requirement**: HIGH (if public endpoint)
**Default**: None (open access) ❌
**Risk**: Unauthorized data access/modification
**Must Set**: YES (for public endpoints)

**Options**:

**Option 1: Reverse Proxy (Recommended)**
```nginx
# nginx.conf
server {
    listen 443 ssl;
    server_name oxigraph.example.com;

    # Basic auth
    auth_basic "Oxigraph";
    auth_basic_user_file /etc/nginx/.htpasswd;

    # Rate limiting
    limit_req_zone $binary_remote_addr zone=oxigraph:10m rate=10r/s;
    limit_req zone=oxigraph burst=20 nodelay;

    location / {
        proxy_pass http://localhost:7878;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    }
}
```

**Option 2: API Gateway**
```yaml
# Kong/AWS API Gateway
- name: oxigraph
  url: http://oxigraph:7878
  plugins:
    - name: key-auth
    - name: rate-limiting
      config:
        minute: 100
        hour: 1000
```

**Option 3: Application-Level** (if embedded)
```rust
// Rocket/Axum middleware
#[rocket::get("/query?<query>")]
async fn query_endpoint(
    user: AuthenticatedUser,  // Validates JWT/API key
    query: String
) -> Result<Json<QueryResults>> {
    // Check authorization
    if !user.can_read() {
        return Err(Status::Forbidden);
    }

    store.query(&query)
}
```

**Must Configure**:
- [ ] Authentication mechanism (API keys, JWT, OAuth)
- [ ] Authorization rules (read-only vs. read-write)
- [ ] Rate limiting (per user/IP)
- [ ] Audit logging (all mutations)

**Verification**:
```bash
# Test authentication required
curl http://localhost:7878/query  # Should return 401

# Test authenticated access
curl -H "Authorization: Bearer $TOKEN" http://localhost:7878/query
```

---

## ✅ AUTOMATIC CONFIGURATIONS (No Action Needed)

These are handled automatically by Oxigraph or the underlying system.

### 1. ACID Transaction Semantics ✅

**Status**: Automatic (RocksDB)
**Action**: None required
**Evidence**: RocksDB provides ACID guarantees

### 2. Query Result Determinism ✅

**Status**: Automatic (ORDER BY guarantees stable sort)
**Action**: None required
**Evidence**: SPARQL evaluation is deterministic

### 3. SHACL Validation Determinism ✅

**Status**: Automatic (rule-based evaluation)
**Action**: None required
**Evidence**: Constraint evaluation is deterministic

### 4. RDF Parser Safety ✅

**Status**: Automatic (chunked parsing, no bombs)
**Action**: None required
**Evidence**: Parsers use streaming, bounded memory

### 5. RocksDB Compaction ✅

**Status**: Automatic (background compaction)
**Action**: None required (default settings reasonable)
**Evidence**: RocksDB manages LSM-tree compaction

### 6. Index Maintenance ✅

**Status**: Automatic (SPO, POS, OSP indexes)
**Action**: None required
**Evidence**: Indexes updated on insert/delete

---

## ⚙️ OPTIONAL CONFIGURATIONS (Tuning)

These configurations are optional but recommended for optimization.

### 1. RocksDB Cache Size 🎛️

**Status**: Optional (defaults to reasonable size)
**Current**: Not exposed in API ⚠️
**Action**: Use defaults (or modify source code)

**Future API** (when available):
```rust
let mut options = StoreOptions::new();
options.set_block_cache_size(2 * 1024 * 1024 * 1024); // 2GB
let store = Store::open_with_options("./data", options)?;
```

**Recommended**: 25% of available memory

---

### 2. Bulk Load Optimization 🚀

**Status**: Optional (use for large imports)
**API**: `bulk_loader()` method
**Action**: Use for initial load or large imports

**Usage**:
```rust
use oxigraph::store::BulkLoader;

let loader = store.bulk_loader()?;
for triple in triples {
    loader.load_triple(triple)?;
}
loader.finish()?;  // Rebuilds indexes
```

**Benefits**:
- 10x faster than individual inserts
- Bypasses indexes during load
- Rebuilds indexes optimally after load

**When to Use**:
- Initial data load (>100K triples)
- Large batch imports
- Database migrations

---

### 3. Transaction Batching 📦

**Status**: Optional (improves write performance)
**API**: Transaction API
**Action**: Batch writes in transactions

**Usage**:
```rust
let mut transaction = store.transaction(Transaction::ReadWrite)?;
for triple in batch {
    transaction.insert(triple)?;
}
transaction.commit()?;
```

**Benefits**:
- Reduces write amplification
- Improves throughput (10x faster than single-triple writes)

**When to Use**:
- Batch updates (>100 triples)
- Import scripts
- Periodic data refreshes

---

### 4. SPARQL Query Optimization Hints 💡

**Status**: Optional (query-specific tuning)
**API**: Query string
**Action**: Use LIMIT, FILTER pushdown, OPTIONAL carefully

**Best Practices**:
```sparql
# Good: LIMIT early
SELECT ?s WHERE {
    ?s rdf:type ex:Person .
} LIMIT 100

# Good: FILTER pushdown
SELECT ?s WHERE {
    ?s rdf:type ex:Person .
    FILTER(STR(?s) = "http://example.org/alice")
}

# Bad: Cartesian product
SELECT ?s1 ?s2 WHERE {
    ?s1 rdf:type ex:Person .
    ?s2 rdf:type ex:Organization .
}  # Explodes with large datasets

# Good: Join condition
SELECT ?s1 ?s2 WHERE {
    ?s1 rdf:type ex:Person .
    ?s1 ex:worksFor ?s2 .
    ?s2 rdf:type ex:Organization .
}
```

---

### 5. Result Set Pagination 📄

**Status**: Optional (recommended for large results)
**API**: LIMIT + OFFSET
**Action**: Paginate large result sets

**Usage**:
```sparql
# Page 1 (first 100 results)
SELECT ?s ?p ?o WHERE {
    ?s ?p ?o .
}
ORDER BY ?s
LIMIT 100
OFFSET 0

# Page 2 (next 100 results)
SELECT ?s ?p ?o WHERE {
    ?s ?p ?o .
}
ORDER BY ?s
LIMIT 100
OFFSET 100
```

**Benefits**:
- Reduces memory usage
- Improves response time
- Better UX (progressive loading)

---

## 🌐 EXTERNAL CONFIGURATIONS (Infrastructure)

These are infrastructure-level configurations outside Oxigraph itself.

### 1. Load Balancer / Reverse Proxy

**Tools**: nginx, HAProxy, Traefik, AWS ELB
**Purpose**: SSL termination, rate limiting, load distribution
**Configuration**: See "Access Control" section above

---

### 2. Service Discovery

**Tools**: Kubernetes Service, Consul, etcd
**Purpose**: Dynamic endpoint discovery
**Configuration**:
```yaml
# Kubernetes Service
apiVersion: v1
kind: Service
metadata:
  name: oxigraph
spec:
  selector:
    app: oxigraph
  ports:
    - protocol: TCP
      port: 7878
      targetPort: 7878
```

---

### 3. Log Aggregation

**Tools**: ELK Stack, Loki, CloudWatch
**Purpose**: Centralized logging, search, analysis
**Configuration**:
```yaml
# Fluentd/Fluent Bit
<source>
  @type tail
  path /var/log/oxigraph/*.log
  tag oxigraph
</source>

<match oxigraph>
  @type elasticsearch
  host elasticsearch
  port 9200
  index_name oxigraph
</match>
```

---

### 4. Distributed Tracing

**Tools**: Jaeger, Zipkin, OpenTelemetry
**Purpose**: Request tracing across services
**Status**: Not built-in (requires application wrapper)

---

## 📋 Pre-Deployment Checklist

### Critical (Must Complete) ✅

- [ ] **Query timeout configured** (30 seconds)
- [ ] **Container memory limits set** (4-8GB)
- [ ] **Monitoring deployed** (Prometheus + Grafana)
- [ ] **Backup strategy implemented** (daily snapshots)
- [ ] **Access control configured** (if public endpoint)
- [ ] **Alerts configured** (latency, errors, memory)
- [ ] **Health check endpoint** (custom wrapper)
- [ ] **Recovery procedure tested** (restore from backup)

### High Priority (Strongly Recommended) ⚠️

- [ ] **Staging deployment** (run soak test 24 hours)
- [ ] **Performance baselines** (measure p50/p95/p99 latency)
- [ ] **Capacity planning** (estimate storage growth)
- [ ] **Rate limiting** (prevent abuse)
- [ ] **Audit logging** (track mutations)
- [ ] **SSL/TLS** (encrypt transport)

### Optional (Nice to Have) 💡

- [ ] **Custom metrics** (application-specific)
- [ ] **Structured logging** (JSON format)
- [ ] **Distributed tracing** (if microservices)
- [ ] **Blue-green deployment** (zero-downtime updates)
- [ ] **Auto-scaling** (horizontal pod autoscaler)

---

## 🚀 Production Readiness Verification

Run this verification script before deployment:

```bash
#!/bin/bash
# production-readiness-check.sh

echo "=== OXIGRAPH PRODUCTION READINESS CHECK ==="

# 1. Query timeout
echo -n "Query timeout configured... "
if grep -q "set_query_timeout\|queryTimeout\|query_timeout" /app/config/*; then
    echo "✅ YES"
else
    echo "❌ NO - CRITICAL"
    exit 1
fi

# 2. Memory limits
echo -n "Container memory limits... "
if docker inspect oxigraph | grep -q "\"Memory\":"; then
    echo "✅ YES"
else
    echo "❌ NO - CRITICAL"
    exit 1
fi

# 3. Monitoring
echo -n "Monitoring endpoint... "
if curl -f http://localhost:7878/metrics > /dev/null 2>&1; then
    echo "✅ YES"
else
    echo "⚠️  NO - HIGH PRIORITY"
fi

# 4. Backup
echo -n "Backup script... "
if [ -f /usr/local/bin/backup-oxigraph.sh ]; then
    echo "✅ YES"
else
    echo "❌ NO - CRITICAL"
    exit 1
fi

# 5. Health check
echo -n "Health check... "
if curl -f http://localhost:7878/health > /dev/null 2>&1; then
    echo "✅ YES"
else
    echo "⚠️  NO - HIGH PRIORITY"
fi

echo ""
echo "=== READY FOR DEPLOYMENT ==="
```

---

## 📚 References

- [Oxigraph Documentation](https://oxigraph.org/)
- [SPARQL 1.1 Specification](https://www.w3.org/TR/sparql11-query/)
- [SHACL Specification](https://www.w3.org/TR/shacl/)
- [RocksDB Tuning Guide](https://github.com/facebook/rocksdb/wiki/RocksDB-Tuning-Guide)
- [Prometheus Best Practices](https://prometheus.io/docs/practices/)

---

**END OF CHECKLIST**
