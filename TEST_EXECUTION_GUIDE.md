# Resource Limits Test Execution Guide

This guide explains how to run and interpret the resource limit enforcement tests for Oxigraph.

---

## Quick Start

```bash
# Navigate to Oxigraph root
cd /home/user/oxigraph

# Run all resource limit tests
cargo test -p oxigraph --test resource_limits

# Run specific test
cargo test -p oxigraph --test resource_limits test_query_timeout_default_behavior_documented

# Run with output
cargo test -p oxigraph --test resource_limits -- --nocapture
```

---

## Test Files

### 1. Test Suite
**Path**: `/home/user/oxigraph/lib/oxigraph/tests/resource_limits.rs`
- **Lines**: 461
- **Tests**: 12
- **Categories**: Query timeout, memory, SHACL, bulk loading, HTTP

### 2. Configuration Documentation
**Path**: `/home/user/oxigraph/CONFIGURATION_DOSSIER.md`
- Complete resource limit configuration guide
- Production deployment checklist
- Code examples for mitigations

### 3. Agent Report
**Path**: `/home/user/oxigraph/AGENT_9_REPORT.md`
- Test suite overview
- Key findings summary
- Recommendations for maintainers

---

## Test List

### Query Timeout Tests

#### ✅ `test_query_cancellation_token_works`
**Purpose**: Verify CancellationToken can stop a query
**Expected**: PASS
**Output**: Query completes or is cancelled
**Evidence**: CancellationToken mechanism works

#### ❌ `test_query_timeout_default_behavior_documented`
**Purpose**: Document that NO default timeout exists
**Expected**: PASS (documents gap)
**Output**: "NO DEFAULT TIMEOUT ENFORCED"
**Evidence**: Query completes without timeout

#### ✅ `test_query_timeout_application_pattern`
**Purpose**: Show recommended timeout pattern
**Expected**: PASS
**Output**: "Application timeout worked"
**Evidence**: CancellationToken + thread pattern

#### ✅ `test_production_timeout_pattern`
**Purpose**: Complete production-ready example
**Expected**: PASS
**Output**: Query completes within timeout
**Evidence**: Helper function demonstrates pattern

---

### Memory Limit Tests

#### ⚠️  `test_memory_limit_not_configurable`
**Purpose**: Document no memory limit API
**Expected**: PASS (documents gap)
**Output**: "memory bounded by iteration, not configuration"
**Evidence**: Results are streamed, not buffered

---

### Result Set Limit Tests

#### ✅ `test_result_set_streaming`
**Purpose**: Verify results are streamed
**Expected**: PASS
**Output**: "Consumed 100 results from stream - rest not materialized"
**Evidence**: Iterator can be partially consumed

#### ✅ `test_result_limit_via_sparql`
**Purpose**: Show LIMIT clause pattern
**Expected**: PASS
**Output**: Assertion passes (count == 10)
**Evidence**: LIMIT clause restricts results

---

### SHACL Validation Tests

#### ✅ `test_shacl_recursion_limit_enforced`
**Purpose**: Document MAX_RECURSION_DEPTH
**Expected**: PASS
**Output**: "SHACL MAX_RECURSION_DEPTH = 50 (hardcoded)"
**Evidence**: Code reference to validator.rs:21

#### ❌ `test_shacl_validation_timeout_not_available`
**Purpose**: Document missing timeout
**Expected**: PASS (documents gap)
**Output**: "SHACL validation has no built-in timeout"
**Evidence**: No timeout parameter in validate()

---

### Bulk Loader Tests

#### ⚠️  `test_bulk_loader_configuration`
**Purpose**: Document available configuration
**Expected**: PASS
**Output**: "BulkLoader has memory/thread config, but no timeout"
**Evidence**: API has num_threads, max_memory_size

---

### HTTP Service Tests

#### ✅ `test_http_service_timeout_configurable`
**Purpose**: Verify HTTP timeout works
**Expected**: PASS (if http-client feature enabled)
**Output**: "HTTP SERVICE timeout is configurable"
**Evidence**: with_http_timeout() API exists

**Note**: This test requires `--features http-client`

---

### Summary Test

#### 📋 `test_resource_limits_summary`
**Purpose**: Print comprehensive summary
**Expected**: PASS (always)
**Output**: Multi-section summary with ✅ and ❌ indicators

---

## Expected Output

### Successful Run

```
running 12 tests
test test_bulk_loader_configuration ... ok
test test_http_service_timeout_configurable ... ok
test test_memory_limit_not_configurable ... ok
test test_production_timeout_pattern ... ok
test test_query_cancellation_token_works ... ok
test test_query_timeout_application_pattern ... ok
test test_query_timeout_default_behavior_documented ... ok
test test_resource_limits_summary ... ok
test test_result_limit_via_sparql ... ok
test test_result_set_streaming ... ok
test test_shacl_recursion_limit_enforced ... ok
test test_shacl_validation_timeout_not_available ... ok

test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.52s
```

### With `--nocapture` Flag

```
running 12 tests

test test_query_timeout_default_behavior_documented ...
Query returned 100 results in 15.2ms - NO DEFAULT TIMEOUT ENFORCED
ok

test test_memory_limit_not_configurable ...
Streamed 1000 results - memory bounded by iteration, not configuration
ok

test test_result_set_streaming ...
Consumed 100 results from stream - rest not materialized
ok

test test_resource_limits_summary ...

=== RESOURCE LIMIT TEST SUMMARY ===

✅ AVAILABLE:
  - CancellationToken for manual query cancellation
  - HTTP SERVICE timeout (federated queries only)
  - Result streaming (memory-bounded iteration)
  - SHACL recursion depth limit (hardcoded: 50)
  - BulkLoader memory/thread configuration

❌ GAPS:
  - No default query timeout
  - No configurable query timeout
  - No query memory limits
  - No SHACL validation timeout
  - No bulk loader timeout

📋 RECOMMENDATIONS:
  1. Implement application-level timeout wrapper (CancellationToken + thread)
  2. Use LIMIT clauses in queries to bound result sets
  3. Set container memory limits (Kubernetes/Docker)
  4. Monitor query execution time and resource usage
  5. Implement rate limiting at application/API layer

=== END SUMMARY ===

ok
```

---

## Interpreting Results

### All Tests Pass (Expected)

**Meaning**:
- Tests verify current behavior (both capabilities AND gaps)
- "PASS" doesn't mean "feature exists", it means "test correctly documents status"

**Example**:
- `test_query_timeout_default_behavior_documented` **PASSES** because it correctly documents that NO timeout exists
- This is a **gap**, not a success

### Test Fails (Unexpected)

If any test fails, it indicates:
1. **API changed**: Resource limit API may have been added/modified
2. **Behavior changed**: Implementation changed enforcement
3. **Test error**: Test itself has a bug

**Action**: Investigate and update test to match current behavior

---

## Running Individual Tests

### Query Timeout Tests Only
```bash
cargo test -p oxigraph --test resource_limits query_timeout
```

### Memory Tests Only
```bash
cargo test -p oxigraph --test resource_limits memory
```

### SHACL Tests Only
```bash
cargo test -p oxigraph --test resource_limits shacl
```

### Production Pattern Test
```bash
cargo test -p oxigraph --test resource_limits test_production_timeout_pattern -- --nocapture
```

---

## Continuous Integration

### CI Configuration

Add to `.github/workflows/test.yml`:

```yaml
- name: Run resource limits tests
  run: |
    cargo test -p oxigraph --test resource_limits -- --nocapture
```

### Expected CI Behavior

- All 12 tests should pass
- Summary output should be visible in CI logs
- Gaps should be documented in CI artifacts

---

## Troubleshooting

### RocksDB Build Issues

If you see:
```
error: couldn't read `oxrocksdb-sys/rocksdb/src.mk`
```

**Solution**:
```bash
git submodule update --init --recursive
```

### Feature Flag Issues

If `test_http_service_timeout_configurable` fails:

**Solution**: Enable feature flag:
```bash
cargo test -p oxigraph --test resource_limits --features http-client
```

### Timeout Tests Flaky

If timeout tests occasionally fail due to timing:

**Cause**: Thread scheduling delays
**Solution**: Increase timeout duration in test
**Note**: This is expected in virtualized/containerized environments

---

## Maintenance

### When to Update Tests

1. **New resource limit added**: Add test verifying enforcement
2. **Existing limit changed**: Update test expectations
3. **Gap closed**: Change test from "documents gap" to "verifies enforcement"

### Test Template

```rust
#[test]
fn test_new_limit_feature() -> Result<(), Box<dyn std::error::Error>> {
    // SETUP: Create store and data

    // TEST: Verify limit is enforced

    // ASSERT: Check expected behavior

    // DOCUMENT: Print status
    eprintln!("Feature X: [status]");

    Ok(())
}
```

---

## Integration with Configuration Dossier

### Test Results → Documentation

Each test finding is documented in `CONFIGURATION_DOSSIER.md`:

| Test | Dossier Section |
|------|----------------|
| `test_query_timeout_*` | Query Execution → Timeout |
| `test_memory_limit_*` | Query Execution → Memory Limit |
| `test_result_set_*` | Query Execution → Result Set Limit |
| `test_shacl_*` | Validation (SHACL) → Timeout/Recursion |
| `test_bulk_loader_*` | Ingestion → Bulk Loader |
| `test_http_service_*` | HTTP SERVICE → Timeout |

---

## Summary

✅ **Test Suite Status**: Complete (12 tests)
✅ **Documentation Status**: Complete (CONFIGURATION_DOSSIER.md)
✅ **Evidence Status**: All limits tested or documented
✅ **Production Guidance**: Mitigations provided

**Next Steps**:
1. Run tests: `cargo test -p oxigraph --test resource_limits`
2. Review output with `--nocapture` flag
3. Read `CONFIGURATION_DOSSIER.md` for production deployment
4. Implement recommended mitigations before production
