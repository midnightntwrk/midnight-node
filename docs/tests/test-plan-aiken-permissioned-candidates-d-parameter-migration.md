# Test Plan: Aiken Permissioned Candidates & D Parameter Migration

**ADR:** [adr-aiken-permissioned-candidates-d-parameter-migration](../decisions/adr-aiken-permissioned-candidates-d-parameter-migration.md)
**Ticket:** [PM-20994](https://shielded.atlassian.net/browse/PM-20994)

---

## Overview

This test plan validates the migration from Haskell-based Permissioned Candidates contracts to Aiken-based contracts, and the transition of D Parameter sourcing from Cardano contracts to `pallet-system-parameters`.

Key changes validated:
1. D Parameter is sourced from [`pallet-system-parameters`](../../pallets/system-parameters/src/lib.rs)
2. [`select_authorities_optionally_overriding`](../../runtime/src/lib.rs#L581) uses pallet storage directly
3. Emergency `DParameterOverride` mechanism removed from `pallet-midnight`

---

## Test Cases

| Test ID | Objective | Steps | Expected Result | Type |
|---|---|---|---|---|
| TC-01 | Verify D Parameter can be updated via pallet extrinsic | 1. Call `SystemParameters::update_d_parameter(Root, 5, 3)` <br>2. Verify storage updated | D Parameter in storage is (5, 3) | Unit |
| TC-02 | Verify `get_d_parameter()` returns current storage values | 1. Set D Parameter via extrinsic <br>2. Call `SystemParameters::get_d_parameter()` | Returns `DParameter` with configured values | Unit |
| TC-03 | Verify authority selection uses pallet D Parameter | 1. Set D Parameter via pallet <br>2. Call authority selection <br>3. Verify correct validator count selected | Authority selection respects pallet D Parameter values | Unit |
| TC-04 | Verify Aura authority rotation continues to work | 1. Run `check_aura_authorities_rotation` test | Test passes, Aura authorities rotate as expected | Unit |
| TC-05 | Verify Grandpa authority rotation continues to work | 1. Run `check_grandpa_authorities_rotation` test | Test passes, Grandpa authorities rotate as expected | Unit |
| TC-06 | Verify cross-chain committee rotation continues to work | 1. Run `check_cross_chain_committee_rotation` test | Test passes, cross-chain committee rotates as expected | Unit |
| TC-07 | Verify D Parameter override integration test | 1. Run `check_overridden_d_param_committee_rotation` test | D Parameter from pallet correctly overrides inherent data | Unit |

---

## Running Tests

```bash
# Run all runtime tests
cargo test -p midnight-node-runtime --lib

# Run pallet-system-parameters tests
cargo test -p pallet-system-parameters --lib

# Run committee rotation tests
cargo test -p midnight-node-runtime --lib check_

# Verify build
cargo build -p midnight-node-runtime
cargo build -p pallet-midnight
cargo build -p pallet-system-parameters
```

---

## Test Coverage

| Component | Test File | Status |
|-----------|-----------|--------|
| pallet-system-parameters | `pallets/system-parameters/src/tests.rs` | ✅ Covered |
| Runtime authority selection | `runtime/src/lib.rs` (tests module) | ✅ Covered |
| D Parameter integration | `check_overridden_d_param_committee_rotation` | ✅ Covered |

