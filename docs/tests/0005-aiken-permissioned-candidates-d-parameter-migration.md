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

| <div style="width:120px">Test ID</div> | <div style="width:350px">Objective</div> | <div style="width:400px">Steps</div> | <div style="width:350px">Expected Result</div> | <div style="width:50px">Type</div> |
|---|---|---|---|---|
| [PR378-TC-01](../../../runtime/src/d_parameter.rs#L78) | Verify mock provider returns `None` to maintain backward compatibility with inherent data | 1. Call `MockDParameterProvider::get_d_parameter()`  <br>2. Verify result is `None` | Returns `None`, indicating inherent data should be used | Unit |
| [PR378-TC-02](../../../runtime/src/d_parameter.rs#L84) | Verify fixed provider returns configured D Parameter values for testing | 1. Create `FixedDParameterProvider<3, 2>` type  <br>2. Call `get_d_parameter()`  <br>3. Verify returned values match configuration | Returns `Some(DParameter)` with `num_permissioned_candidates = 3`, `num_registered_candidates = 2` | Unit |
| [PR378-TC-03](../../../runtime/src/lib.rs#L1730) | Verify `select_authorities_with_provider` correctly uses provider values | 1. Create authority selection inputs with D=(1, 0)  <br>2. Call `select_authorities_with_provider::<FixedDParameterProvider<20, 2>>`  <br>3. Verify D Parameter override is applied | Provider overrides input D Parameter; authority selection uses provider values (20 permissioned, 2 registered) | Unit |
| [PR378-TC-04](../../../runtime/src/lib.rs#L1671) | Verify Aura authority rotation continues to work | 1. Run existing `check_aura_authorities_rotation` test  <br>2. Verify authorities rotate correctly | Test passes, Aura authorities rotate as expected | Unit |
| [PR378-TC-05](../../../runtime/src/lib.rs#L1620) | Verify Grandpa authority rotation continues to work | 1. Run existing `check_grandpa_authorities_rotation` test  <br>2. Verify authorities rotate correctly | Test passes, Grandpa authorities rotate as expected | Unit |
| [PR378-TC-06](../../../runtime/src/lib.rs#L1705) | Verify cross-chain committee rotation continues to work | 1. Run existing `check_cross_chain_committee_rotation` test  <br>2. Verify committee rotates correctly | Test passes, cross-chain committee rotates as expected | Unit |

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
