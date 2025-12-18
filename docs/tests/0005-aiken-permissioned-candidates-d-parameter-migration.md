# Test Plan: Aiken Permissioned Candidates & D Parameter Migration

**ADR:** [0005-aiken-permissioned-candidates-d-parameter-migration](../decisions/0005-aiken-permissioned-candidates-d-parameter-migration.md)
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

| Test ID     | Name | Objective | Steps | Expected Result | Location | Type |
|-------------|------|-----------|-------|-----------------|----------|------|
| PR378-TC-01 | MockDParameterProvider Returns None | Verify mock provider returns `None` to maintain backward compatibility with inherent data | 1. Call `MockDParameterProvider::get_d_parameter()`  <br>2. Verify result is `None` | Returns `None`, indicating inherent data should be used | `runtime/src/d_parameter.rs` - `mock_provider_returns_none` | Unit |
| PR378-TC-02 | FixedDParameterProvider Returns Configured Values | Verify fixed provider returns configured D Parameter values for testing | 1. Create `FixedDParameterProvider<3, 2>` type  <br>2. Call `get_d_parameter()`  <br>3. Verify returned values match configuration | Returns `Some(DParameter)` with `num_permissioned_candidates = 3`, `num_registered_candidates = 2` | `runtime/src/d_parameter.rs` - `fixed_provider_returns_configured_values` | Unit |
| PR378-TC-03 | Provider Integration with Authority Selection | Verify `select_authorities_with_provider` correctly uses provider values | 1. Create authority selection inputs with D=(1, 0)  <br>2. Call `select_authorities_with_provider::<FixedDParameterProvider<20, 2>>`  <br>3. Verify D Parameter override is applied | Provider overrides input D Parameter; authority selection uses provider values (20 permissioned, 2 registered) | `runtime/src/lib.rs` - `check_d_parameter_provider_integration` | Unit |
| PR378-TC-04 | Aura Authorities Rotation Not Affected | Verify Aura authority rotation continues to work | 1. Run existing `check_aura_authorities_rotation` test  <br>2. Verify authorities rotate correctly | Test passes, Aura authorities rotate as expected | `runtime/src/lib.rs` - `check_aura_authorities_rotation` | Unit |
| PR378-TC-05 | Grandpa Authorities Rotation Not Affected | Verify Grandpa authority rotation continues to work | 1. Run existing `check_grandpa_authorities_rotation` test  <br>2. Verify authorities rotate correctly | Test passes, Grandpa authorities rotate as expected | `runtime/src/lib.rs` - `check_grandpa_authorities_rotation` | Unit |
| PR378-TC-06 | Cross-Chain Committee Rotation Not Affected | Verify cross-chain committee rotation continues to work | 1. Run existing `check_cross_chain_committee_rotation` test  <br>2. Verify committee rotates correctly | Test passes, cross-chain committee rotates as expected | `runtime/src/lib.rs` - `check_cross_chain_committee_rotation` | Unit |

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

- **ADR:** [0005-aiken-permissioned-candidates-d-parameter-migration](../decisions/0005-aiken-permissioned-candidates-d-parameter-migration.md)
- **Implementation:** `runtime/src/d_parameter.rs` - `DParameterProvider` trait
- **Integration:** `runtime/src/lib.rs` - `select_authorities_with_provider`
