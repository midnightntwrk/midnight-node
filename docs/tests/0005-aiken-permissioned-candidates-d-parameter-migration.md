# Test Plan: Aiken Permissioned Candidates & D Parameter Migration

**ADR:** [0004-aiken-permissioned-candidates-d-parameter-migration](../decisions/0004-aiken-permissioned-candidates-d-parameter-migration.md)
**Ticket:** [PM-20994](https://shielded.atlassian.net/browse/PM-20994)
**PR:** [#378](https://github.com/midnightntwrk/midnight-node/pull/378)

---

## Overview

This test plan validates the migration from Haskell-based Permissioned Candidates contracts to Aiken-based contracts, and the transition of D Parameter sourcing from Cardano contracts to `pallet-system-parameters` (via mock initially).

Key changes validated:
1. `DParameterProvider` trait correctly abstracts D Parameter sourcing
2. `MockDParameterProvider` maintains backward compatibility (uses inherent data)
3. Authority selection continues to work correctly

---

## Test Cases

### PR378-TC-01: MockDParameterProvider Returns None

**Objective:** Verify mock provider returns `None` to maintain backward compatibility with inherent data.

**Steps:**
1. Call `MockDParameterProvider::get_d_parameter()`
2. Verify result is `None`

**Expected Result:**
- Returns `None`, indicating inherent data should be used

**Test Location:** `runtime/src/d_parameter.rs` - `mock_provider_returns_none`

---

### PR378-TC-02: FixedDParameterProvider Returns Configured Values

**Objective:** Verify fixed provider returns configured D Parameter values for testing.

**Steps:**
1. Create `FixedDParameterProvider<3, 2>` type
2. Call `get_d_parameter()`
3. Verify returned values match configuration

**Expected Result:**
- Returns `Some(DParameter)` with `num_permissioned_candidates = 3`, `num_registered_candidates = 2`

**Test Location:** `runtime/src/d_parameter.rs` - `fixed_provider_returns_configured_values`

---

### PR378-TC-03: Provider Integration with Authority Selection

**Objective:** Verify `select_authorities_with_provider` correctly uses provider values.

**Steps:**
1. Create authority selection inputs with D=(1, 0)
2. Call `select_authorities_with_provider::<FixedDParameterProvider<20, 2>>`
3. Verify D Parameter override is applied

**Expected Result:**
- Provider overrides input D Parameter
- Authority selection uses provider values (20 permissioned, 2 registered)

**Test Location:** `runtime/src/lib.rs` - `check_d_parameter_provider_integration`

---

### PR378-TC-04: Aura Authorities Rotation Not Affected

**Objective:** Verify Aura authority rotation continues to work.

**Steps:**
1. Run existing `check_aura_authorities_rotation` test
2. Verify authorities rotate correctly

**Expected Result:**
- Test passes, Aura authorities rotate as expected

**Test Location:** `runtime/src/lib.rs` - `check_aura_authorities_rotation`

---

### PR378-TC-05: Grandpa Authorities Rotation Not Affected

**Objective:** Verify Grandpa authority rotation continues to work.

**Steps:**
1. Run existing `check_grandpa_authorities_rotation` test
2. Verify authorities rotate correctly

**Expected Result:**
- Test passes, Grandpa authorities rotate as expected

**Test Location:** `runtime/src/lib.rs` - `check_grandpa_authorities_rotation`

---

### PR378-TC-06: Cross-Chain Committee Rotation Not Affected

**Objective:** Verify cross-chain committee rotation continues to work.

**Steps:**
1. Run existing `check_cross_chain_committee_rotation` test
2. Verify committee rotates correctly

**Expected Result:**
- Test passes, cross-chain committee rotates as expected

**Test Location:** `runtime/src/lib.rs` - `check_cross_chain_committee_rotation`

---

## Test Matrix

| Test Case | Unit | Notes |
|-----------|------|-------|
| PR378-TC-01 | ✅ | `mock_provider_returns_none` |
| PR378-TC-02 | ✅ | `fixed_provider_returns_configured_values` |
| PR378-TC-03 | ✅ | `check_d_parameter_provider_integration` |
| PR378-TC-04 | ✅ | `check_aura_authorities_rotation` |
| PR378-TC-05 | ✅ | `check_grandpa_authorities_rotation` |
| PR378-TC-06 | ✅ | `check_cross_chain_committee_rotation` |

Legend: ✅ Pass | ❌ Fail | ⬜ Not Started

---

## Running Tests

```bash
# Run all runtime tests
cargo test -p midnight-node-runtime --lib

# Run d_parameter module tests specifically
cargo test -p midnight-node-runtime --lib d_parameter

# Run committee rotation tests
cargo test -p midnight-node-runtime --lib check_

# Verify build
cargo build -p midnight-node-runtime
cargo build -p pallet-midnight
```

---

## References

- **ADR:** [0004-aiken-permissioned-candidates-d-parameter-migration](../decisions/0004-aiken-permissioned-candidates-d-parameter-migration.md)
- **Implementation:** `runtime/src/d_parameter.rs` - `DParameterProvider` trait
- **Integration:** `runtime/src/lib.rs` - `select_authorities_with_provider`
