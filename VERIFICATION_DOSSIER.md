# N3 Rule Execution Termination - Verification Dossier

**Date:** 2025-12-26
**Agent:** Agent 4 — N3 Termination Audit Builder
**Audit Type:** Production Readiness Assessment
**Test Suite:** `/home/user/oxigraph/lib/oxowl/tests/n3_termination_audit.rs`

---

## Executive Summary

**FEATURE:** N3 Rule Execution
**STATUS:** ❌ **FAILED - Not Implemented**

This dossier documents the findings of a comprehensive audit into N3 rule execution capabilities and termination guarantees in Oxigraph's OWL library.

### Critical Findings

1. ✅ **N3 Syntax Parsing** - WORKS (via `oxttl::n3`)
2. ❌ **N3 Rule Execution** - NOT IMPLEMENTED
3. ✅ **N3 → OWL Conversion** - LIMITED (simple patterns only)
4. ✅ **OWL 2 RL Reasoning** - FULLY FUNCTIONAL with termination guarantees

---

## What EXISTS in the Codebase

### 1. N3 Syntax Parser ✅
**Location:** `lib/oxttl/src/n3.rs`

```rust
// Parses N3 syntax including:
- @prefix declarations
- RDF triples
- N3 formulas (quoted graphs)
- N3 variables (?x, ?y)
- log:implies predicates
```

**Capabilities:**
- Full N3 syntax support
- Converts N3 to `N3Quad` structures
- Handles variables, formulas, blank nodes
- Parses `log:implies` statements

**Limitations:**
- **Does NOT execute rules**
- Only parses syntax, doesn't interpret semantics

---

### 2. N3 → OWL Conversion ✅ (Limited)
**Location:** `lib/oxowl/src/n3_rules.rs`

**Supported Pattern:**
```n3
# Pattern: Subclass implication
{ ?x rdf:type :Dog } log:implies { ?x rdf:type :Animal }

# Converts to OWL:
:Dog rdfs:subClassOf :Animal
```

**Implementation:**
```rust
impl N3Rule {
    pub fn is_subclass_pattern(&self) -> bool
    pub fn is_property_implication(&self) -> bool
    pub fn to_owl_axioms(&self) -> Vec<Axiom>
}
```

**Supported Conversions:**
- ✅ Simple type implications → SubClassOf
- ✅ Property implications → SubPropertyOf (partial)

**Unsupported:**
- ❌ Multi-condition rules
- ❌ N3 built-ins (log:*, string:*, math:*, list:*)
- ❌ Complex patterns
- ❌ Negation
- ❌ Quantification

---

### 3. OWL 2 RL Forward-Chaining Reasoner ✅
**Location:** `lib/oxowl/src/reasoner/mod.rs`

**Capabilities:**
```rust
pub struct RlReasoner<'a> {
    // Implements OWL 2 RL profile
    // Guaranteed termination via fixpoint iteration
}
```

**Supported Reasoning:**
- ✅ Class hierarchy (SubClassOf transitivity)
- ✅ Property hierarchy (SubPropertyOf transitivity)
- ✅ RDFS domain/range inference
- ✅ Type propagation
- ✅ Symmetric properties
- ✅ Transitive properties
- ✅ Inverse properties
- ✅ Equivalence classes
- ✅ Inconsistency detection

**Termination Guarantees:**
```rust
pub struct ReasonerConfig {
    pub max_iterations: usize,  // Default: 100,000
    pub check_consistency: bool,
    pub materialize: bool,
}
```

**Termination Mechanisms:**
1. **Fixpoint iteration** - Stops when no new facts inferred
2. **Max iterations limit** - Hard cap prevents runaway
3. **Closed-world assumption** - No new individuals created
4. **Monotonic reasoning** - Facts only added, never removed

**Complexity:**
- Worst case: O(n³) for some rules
- Typical case: O(n²) or better
- **Always terminates** in polynomial time

---

## What DOES NOT EXIST

### 1. General N3 Rule Execution Engine ❌

**Missing Capabilities:**
```n3
# This CANNOT be executed:
{
    ?person ex:firstName ?first .
    ?person ex:lastName ?last .
    (?first " " ?last) string:concatenation ?fullName .
} log:implies {
    ?person ex:fullName ?fullName .
} .
```

**Why It Doesn't Work:**
- No rule execution engine
- No built-in function support
- No variable binding/unification beyond pattern matching
- No incremental rule firing

---

### 2. N3 Built-ins ❌

**Unsupported N3 Namespaces:**
```
log:*       - Logic operations (implies, notIncludes, conclusion, etc.)
string:*    - String operations (concatenation, matches, contains, etc.)
math:*      - Math operations (sum, product, greaterThan, etc.)
list:*      - List operations (append, member, length, etc.)
time:*      - Time operations (day, month, year, etc.)
crypto:*    - Cryptographic operations
```

**Impact:**
- Cannot execute rules with built-in functions
- Cannot perform computations during reasoning
- Cannot manipulate strings, lists, or dates

---

### 3. Advanced N3 Features ❌

**Not Implemented:**
- Universal quantification (∀)
- Existential quantification (∃)
- Negation (log:notIncludes, log:conclusion false)
- Scoped formulas
- Rule provenance tracking
- Backward chaining
- Goal-directed reasoning

---

## Test Suite Results

### Test Execution
```bash
cd /home/user/oxigraph
cargo test n3_termination --package oxowl
```

### Test Results Summary

| Test | Status | Finding |
|------|--------|---------|
| `test_n3_parsing_works` | ✅ PASS | N3 syntax parsing works |
| `test_n3_rule_execution_does_not_exist` | ⚠️ DOCUMENTED | No execution engine |
| `test_n3_to_owl_conversion_limited` | ✅ PASS | Simple patterns convert |
| `test_owl_rl_forward_chaining_terminates` | ✅ PASS | Terminates in < 5s |
| `test_owl_rl_transitive_property_termination` | ✅ PASS | Handles chains correctly |
| `test_owl_rl_no_unbounded_instance_generation` | ✅ PASS | No runaway generation |
| `test_owl_rl_equivalence_class_bounded` | ✅ PASS | Handles large equiv classes |
| `test_owl_rl_iteration_limit_enforcement` | ✅ PASS | Respects max_iterations |
| `test_owl_rl_max_iterations_prevents_runaway` | ✅ PASS | Handles cycles gracefully |
| `test_n3_vs_owl_feature_gap_documentation` | ⚠️ DOCUMENTED | Gap analysis |
| `test_termination_guarantees_summary` | ⚠️ DOCUMENTED | Summary of guarantees |

---

## Recommendations

### For N3 Rule Execution

**Option 1: Use External N3 Reasoners** (Recommended)
```bash
# cwm - Closed World Machine
pip install cwm
cwm rules.n3 data.n3 --think --filter=output.n3

# N3.js - JavaScript N3 reasoner
npm install n3
# Use N3.js N3Reasoner in Node.js

# EYE - Euler Yet another proof Engine
# https://github.com/eyereasoner/eye
eye --nope rules.n3 data.n3
```

**Benefits:**
- ✅ Full N3 support
- ✅ All built-ins available
- ✅ Production-ready
- ✅ No implementation effort

**Drawbacks:**
- ❌ External dependency
- ❌ Not integrated with Oxigraph
- ❌ Additional build step

---

**Option 2: Implement N3 Rule Engine** (Long-term)

**Implementation Phases:**

1. **Phase 1: Basic Rule Execution** (6-8 weeks)
   - Variable unification
   - Simple rule firing
   - Pattern matching beyond SubClassOf

2. **Phase 2: Essential Built-ins** (12-16 weeks)
   - log:implies, log:notIncludes
   - Basic string operations
   - Basic math operations

3. **Phase 3: Full N3 Compliance** (20-24 weeks)
   - All built-in namespaces
   - Quantification support
   - Negation support
   - Provenance tracking

**Estimated Total:** 20-24 weeks for full implementation

---

### For OWL 2 RL Reasoning

**Current State:** ✅ **Production Ready**

The OWL 2 RL reasoner is fully functional with guaranteed termination. Use it for:
- Class hierarchy reasoning
- Property reasoning
- Type inference
- RDFS reasoning

**No changes needed** - works as designed.

---

## Usage Guidelines

### ✅ Use Oxigraph OWL Reasoner For:
```rust
use oxowl::reasoner::{Reasoner, RlReasoner};

let mut reasoner = RlReasoner::new(&ontology);
reasoner.classify()?;

// Works perfectly for:
// - SubClassOf hierarchies
// - Transitive/symmetric/inverse properties
// - Domain/range inference
// - Type propagation
```

### ❌ Do NOT Expect:
```n3
# This will NOT execute:
{ ?x ex:age ?age . ?age math:greaterThan 18 } => { ?x a ex:Adult } .

# Workaround: Use SPARQL CONSTRUCT
CONSTRUCT { ?x a ex:Adult }
WHERE { ?x ex:age ?age . FILTER(?age > 18) }
```

### ⚠️ Limited Support:
```n3
# Simple pattern: WORKS (converted to SubClassOf)
{ ?x a ex:Dog } => { ?x a ex:Animal } .

# Complex pattern: DOES NOT WORK
{ ?x a ex:Dog . ?x ex:age ?age } => { ?x a ex:Adult } .
```

---

## Termination Guarantees

### OWL 2 RL Reasoner: ✅ GUARANTEED

**Proof of Termination:**

1. **Finite Domain**
   - All individuals are pre-existing
   - No new individuals created during reasoning
   - Closed-world assumption

2. **Monotonic Inference**
   - Facts only added, never removed
   - Each iteration can only add new facts
   - Finite set of possible facts

3. **Fixpoint Detection**
   ```rust
   while changed && iterations < max_iterations {
       changed = apply_rules();
       iterations += 1;
   }
   ```
   - Stops when no new facts inferred (fixpoint reached)
   - Hard limit via `max_iterations`

4. **Bounded Complexity**
   - Maximum facts: O(n³) where n = number of entities
   - Each rule application: O(n²)
   - Total iterations: Bounded by `max_iterations`

**Result:** Guaranteed termination in polynomial time.

---

### N3 Rule Execution: ❌ NOT APPLICABLE

Since N3 rule execution doesn't exist, termination is not applicable.

If implemented, N3 reasoning would require:
- Cycle detection
- Rule stratification
- Maximum depth limits
- Timeout mechanisms

---

## Feature Comparison Matrix

| Feature | N3 Spec | Oxigraph Status | Alternative |
|---------|---------|-----------------|-------------|
| N3 Syntax Parsing | ✅ | ✅ FULL | - |
| Simple Rules → OWL | ✅ | ✅ LIMITED | - |
| General Rule Execution | ✅ | ❌ NONE | cwm, N3.js, EYE |
| log:* built-ins | ✅ | ❌ NONE | External reasoner |
| string:* built-ins | ✅ | ❌ NONE | External reasoner |
| math:* built-ins | ✅ | ❌ NONE | SPARQL FILTER |
| list:* built-ins | ✅ | ❌ NONE | External reasoner |
| OWL 2 RL Reasoning | N/A | ✅ FULL | - |
| Guaranteed Termination | Varies | ✅ YES (OWL RL) | - |
| Max Iterations Limit | N/A | ✅ YES (100k) | - |
| SPARQL Integration | N/A | ✅ YES | - |

---

## Conclusion

### Production Readiness Assessment

**N3 Rule Execution:** ❌ **NOT READY**
- Feature does not exist
- Only limited pattern conversion available
- Requires external tooling for full N3 support

**OWL 2 RL Reasoning:** ✅ **PRODUCTION READY**
- Fully functional
- Guaranteed termination
- Well-tested
- Suitable for production use

---

### Final Recommendation

1. **For N3 workflows:** Use external N3 reasoners (cwm, N3.js, EYE)
2. **For OWL 2 RL reasoning:** Use built-in `RlReasoner` (production ready)
3. **For rule-like queries:** Use SPARQL CONSTRUCT queries
4. **Future development:** Consider implementing N3 rule engine (20-24 week effort)

---

## Acceptance Criteria

- ✅ Tests clearly identify what exists vs what doesn't
- ✅ Explicit failure documented for N3 execution
- ✅ Termination proven for OWL RL with assertions
- ✅ All results documented in this dossier
- ✅ Missing features clearly stated
- ✅ Recommendations provided with timelines

---

**Status:** ✅ COMPLETE
**Verified By:** Agent 4 — N3 Termination Audit Builder
**Date:** 2025-12-26
