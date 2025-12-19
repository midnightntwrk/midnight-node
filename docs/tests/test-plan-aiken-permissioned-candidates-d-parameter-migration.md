# Test Plan: Aiken Permissioned Candidates & D Parameter Migration

**ADR:** [adr-aiken-permissioned-candidates-d-parameter-migration](../decisions/adr-aiken-permissioned-candidates-d-parameter-migration.md)
**Ticket:** [PM-20994](https://shielded.atlassian.net/browse/PM-20994)
**PR:** [#378](https://github.com/midnightntwrk/midnight-node/pull/378)

---

## Overview

This test plan validates the migration from Haskell-based Permissioned Candidates contracts to Aiken-based contracts, and the transition of D Parameter sourcing from Cardano contracts to `pallet-system-parameters`.

Key changes validated:
1. [`get_d_parameter`](../../pallets/system-parameters/src/lib.rs#L243) returns D Parameter from on-chain storage
2. [`select_authorities_optionally_overriding`](../../runtime/src/lib.rs#L581) uses pallet storage directly
3. Emergency `DParameterOverride` mechanism removed from `pallet-midnight`

---

## Test Cases

| <div style="width:120px">Test ID</div> | <div style="width:350px">Objective</div> | <div style="width:400px">Steps</div> | <div style="width:350px">Expected Result</div> | <div style="width:50px">Type</div> |
|---|---|---|---|---|
| [PR378-TC-01](../../pallets/system-parameters/src/tests.rs#L75) | Verify D Parameter can be updated via pallet extrinsic | 1. Call `SystemParameters::update_d_parameter(Root, 10, 5)`  <br>2. Verify storage updated  <br>3. Check event emitted | D Parameter in storage is (10, 5) and `DParameterUpdated` event emitted | Unit |
| [PR378-TC-02](../../pallets/system-parameters/src/tests.rs#L127) | Verify D Parameter initializes from genesis config | 1. Configure genesis with D Parameter values (15, 10)  <br>2. Build genesis  <br>3. Call `get_d_parameter()` | Returns `DParameter` with genesis-configured values | Unit |
| [PR378-TC-03](../../runtime/src/lib.rs#L1752) | Verify authority selection uses pallet D Parameter | 1. Set D Parameter via pallet  <br>2. Call authority selection  <br>3. Verify correct validator count selected | Authority selection respects pallet D Parameter values, overriding inherent data | Unit |
| [PR378-TC-04](../../runtime/src/lib.rs#L1693) | Verify Aura authority rotation continues to work | 1. Configure committee in session  <br>2. Advance to next session  <br>3. Verify Aura authorities updated | Aura authorities rotate as expected | Unit |
| [PR378-TC-05](../../runtime/src/lib.rs#L1642) | Verify Grandpa authority rotation continues to work | 1. Configure committee in session  <br>2. Advance to next session  <br>3. Verify Grandpa authorities updated | Grandpa authorities rotate as expected | Unit |
| [PR378-TC-06](../../runtime/src/lib.rs#L1727) | Verify cross-chain committee rotation continues to work | 1. Configure committee in session  <br>2. Advance to next session  <br>3. Verify cross-chain committee updated | Cross-chain committee rotates as expected | Unit |
| PR378-TC-07 | Verify `systemParameters_getAriadneParameters` RPC endpoint | 1. Start node with D Parameter configured  <br>2. Call RPC endpoint with epoch number  <br>3. Verify response structure | Response contains D Parameter from pallet (not Cardano) and candidate data from Cardano | Manual |
| PR378-TC-08 | Verify D Parameter override in live network | 1. Deploy to testnet  <br>2. Update D Parameter via governance  <br>3. Observe authority selection in next epoch | Committee size reflects updated D Parameter values | Manual |

---

## Running Tests

```bash
# Run all pallet-system-parameters tests
cargo test -p pallet-system-parameters --lib

# Run all runtime tests
cargo test -p midnight-node-runtime --lib

# Run specific D Parameter override test
cargo test -p midnight-node-runtime --lib check_overridden_d_param

# Run authority rotation tests
cargo test -p midnight-node-runtime --lib check_aura_authorities_rotation
cargo test -p midnight-node-runtime --lib check_grandpa_authorities_rotation
cargo test -p midnight-node-runtime --lib check_cross_chain_committee_rotation

# Verify builds
cargo build -p midnight-node-runtime
cargo build -p pallet-system-parameters
cargo build -p pallet-system-parameters-rpc
```
