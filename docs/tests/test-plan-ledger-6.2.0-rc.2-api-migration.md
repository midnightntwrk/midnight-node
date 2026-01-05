# Test Plan: Ledger 6.2.0-rc.2 API Migration

**ADR:** [0004-ledger-6.2.0-rc.2-api-migration](../decisions/0004-ledger-6.2.0-rc.2-api-migration.md)
**Ticket:** [PM-20907](https://shielded.atlassian.net/browse/PM-20907)
**PR:** [#352](https://github.com/midnightntwrk/midnight-node/pull/352)

---

## Overview

This test plan validates the migration to midnight-ledger version `6.2.0-rc.2`, which introduces breaking API changes to `LedgerState`, storage APIs, and `FeePrices`.

Key changes validated:
1. [`post_block_update`](../../ledger/helpers/src/versions/common/context.rs#L152) accepts new 3-argument signature (`Timestamp`, `NormalizedCost`, `FixedPoint`)
2. [`pre_fetch`](../../ledger/src/versions/common/mod.rs#L125) uses `&ArenaHash` instead of `&ArenaKey`
3. [`FeePrices`](../../util/toolkit/src/commands/update_ledger_parameters.rs#L143) migrated from `*_price` to `*_factor` field naming with new `overall_price` field

---

## Test Cases

| <div style="width:140px">Test ID</div> | <div style="width:300px">Objective</div> | <div style="width:350px">Steps</div> | <div style="width:300px">Expected Result</div> | <div style="width:50px">Type</div> |
|---|---|---|---|---|
| [PR352-TC-01](../../ledger/src/storage.rs#L95) | Verify storage set and drop works with new API | 1. Create temp storage path  <br>2. Set default storage  <br>3. Drop storage | Storage creates and drops without error | Unit |
| [PR352-TC-02](../../ledger/src/versions/common/api/ledger.rs#L312) | Verify LedgerState serialization with new format | 1. Create LedgerState  <br>2. Convert to bytes  <br>3. Convert back from bytes | Round-trip serialization succeeds | Unit |
| [PR352-TC-03](../../ledger/src/versions/common/api/transaction.rs#L469) | Verify malformed transaction deserialization fails | 1. Create invalid transaction bytes  <br>2. Attempt deserialization | Panics as expected (invalid data rejected) | Unit |
| [PR352-TC-04](../../ledger/src/versions/common/api/transaction.rs#L477) | Verify malformed transaction validation fails | 1. Create malformed transaction  <br>2. Attempt validation | Panics as expected (validation rejects) | Unit |
| PR352-TC-05 | Verify node builds with ledger 6.2.0-rc.2 | 1. Run `cargo build --release`  <br>2. Verify no compilation errors | Build succeeds | Build |
| PR352-TC-06 | Verify runtime builds with new API | 1. Run `cargo check -p midnight-node-runtime`  <br>2. Verify no errors | Check succeeds | Build |
| PR352-TC-07 | Verify dev node starts and produces blocks | 1. Start node with `--dev` flag  <br>2. Wait for block production  <br>3. Verify logs | "Prepared block for proposing" appears in logs | Integration |
| PR352-TC-08 | Verify genesis loads without state root mismatch | 1. Start node with regenerated genesis  <br>2. Check for state root errors | Node starts without state root mismatch | Integration |
| [PR352-TC-09](../../ledger/src/versions/common/api/transaction.rs#L455)** | Verify transaction validation works | 1. Load valid transaction fixture  <br>2. Call validate  <br>3. Verify success | Transaction validates successfully | Unit |
| [PR352-TC-10](../../ledger/src/versions/common/api/transaction.rs#L484)** | Verify identifier extraction from transaction | 1. Load transaction fixture  <br>2. Extract identifiers  <br>3. Verify extracted values | Identifiers extracted correctly | Unit |
| [PR352-TC-11](../../ledger/src/versions/common/api/transaction.rs#L503)** | Verify parameter retrieval from transaction | 1. Load transaction fixture  <br>2. Get parameters  <br>3. Verify values | Parameters retrieved correctly | Unit |
| [PR352-TC-12](../../ledger/src/versions/common/api/ledger.rs#L326)** | Verify transaction application to ledger state | 1. Create ledger state  <br>2. Apply transaction  <br>3. Verify state update | State updated correctly | Unit |
| [PR352-TC-13](../../ledger/src/versions/common/api/ledger.rs#L339)** | Verify contract state retrieval | 1. Create ledger with contract  <br>2. Query contract state  <br>3. Verify result | Contract state retrieved correctly | Unit |

> [!NOTE] 
> Tests marked with ** are temporarily ignored pending fixture regeneration. These will be re-enabled once midnight-js is updated and fixtures can be regenerated via `earthly +rebuild-genesis-state --NETWORK=undeployed`.

---

## Running Tests

```bash
# Run all ledger tests (includes ignored tests as skipped)
cargo test -p mn-ledger --lib

# Run only active tests (excluding ignored)
cargo test -p mn-ledger --lib -- --skip "should_validate_transaction" --skip "should_extract_identifiers" --skip "should_get_parameters" --skip "should_apply_transaction" --skip "should_get_contract_state"

# Verify builds
cargo build --release
cargo check -p midnight-node-runtime

# Run toolkit e2e test
./scripts/tests/toolkit-update-ledger-parameters-e2e.sh
```

---

## Manual Verification Procedures

### Integration Tests (PR352-TC-07, PR352-TC-08)

| Step | Action | Expected Outcome |
|------|--------|------------------|
| 1 | Build the node: `cargo build --release` | Build succeeds |
| 2 | Start dev node: `./target/release/midnight-node --dev` | Node starts on ws://127.0.0.1:9944 |
| 3 | Wait for block production | Logs show "Prepared block for proposing" |
| 4 | Check for state root errors | No "state root mismatch" in logs |
| 5 | Stop node (Ctrl+C) | Clean shutdown |

### Fixture Regeneration (Future)

When midnight-js is updated, regenerate fixtures:

```bash
earthly +rebuild-genesis-state --NETWORK=undeployed
```

Then remove `#[ignore]` annotations from tests PR352-TC-09 through PR352-TC-13.

