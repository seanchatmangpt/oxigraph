# Adversarial DoS Security Test Suite

## Overview

This test suite (`adversarial_dos.rs`) verifies Oxigraph's resilience against Denial of Service (DoS) attack vectors.

## Running Tests

```bash
# Run all DoS security tests with output
cargo test --test adversarial_dos -- --nocapture --test-threads=1

# Run specific test
cargo test --test adversarial_dos test_property_path_memory_limit -- --nocapture

# Run with timeout feature enabled
cargo test --test adversarial_dos --features rdfc-10 -- --nocapture
```

## Test Vectors

| Test | Attack Vector | Expected Behavior |
|------|---------------|-------------------|
| `test_property_path_memory_limit` | SPARQL `*` operator on dense graph | Times out or completes with bounded memory |
| `test_canonicalization_blank_node_bomb` | Exponential canonicalization | Completes in <5s or rejects |
| `test_shacl_regex_dos_protection` | Catastrophic backtracking | Protected (Rust regex is safe) |
| `test_query_timeout_default_behavior` | Runaway queries | Documents need for timeout wrapper |
| `test_large_result_set_bounded` | Memory exhaustion | Streaming prevents OOM |
| `test_parser_buffer_overflow_protected` | Large file DoS | Chunked parsing prevents OOM |
| `test_deeply_nested_query_protection` | Stack overflow | Heap-allocated AST safe |
| `test_concurrent_query_resource_limits` | Query flooding | Documents need for rate limiting |

## Interpreting Results

### PASS
Test demonstrates protection exists or documents expected behavior.

### FAIL
Indicates potential vulnerability requiring investigation.

## Production Requirements

Based on test results, production deployments MUST:

1. **Implement query timeout wrapper:**
   ```rust
   let token = CancellationToken::new();
   // Set up timeout watchdog
   SparqlEvaluator::new()
       .with_cancellation_token(token)
       .parse_query(query)?
       .on_store(&store)
       .execute()?
   ```

2. **Implement concurrency limiting:**
   ```rust
   use tokio::sync::Semaphore;
   let semaphore = Semaphore::new(10); // Max 10 concurrent queries
   ```

3. **Configure system limits:**
   - CPU: `ulimit -t 300`
   - Memory: `ulimit -v 8388608`
   - Deploy in containers with resource limits

## Documentation

See `/home/user/oxigraph/SECURITY_DOS_VERIFICATION.md` for detailed analysis of each vector.

## Feature Flags

- `rdfc-10`: Enables RDF canonicalization test
- `shacl`: Enables SHACL regex test (currently manual verification)

## Contact

Report security issues via GitHub Security Advisories, not public issues.
