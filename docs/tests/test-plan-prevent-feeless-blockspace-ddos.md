# Test Plan: Pre-Dispatch Validation of Guaranteed Transaction Part

**ADR:** [0003-prevent-feeless-blockspace-ddos](../decisions/0003-prevent-feeless-blockspace-ddos.md)
**Ticket:** [PM-20944](https://shielded.atlassian.net/browse/PM-20944)
**PR:** [#367](https://github.com/midnightntwrk/midnight-node/pull/367)

---

## Overview

This test plan validates the DDoS mitigation implemented in ADR-0003. The fix adds `validate_guaranteed_execution` to `pre_dispatch` to reject transactions whose guaranteed part would fail before they consume blockspace.

Key changes validated:
1. [`validate_guaranteed_execution`](../../ledger/src/versions/common/api/ledger.rs#L208) - Simulates guaranteed part execution without modifying state
2. [`pre_dispatch`](../../pallets/midnight/src/lib.rs#L458) - Enhanced to call `validate_guaranteed_execution` before block inclusion

---

## Test Cases

| <div style="width:120px">Test ID</div> | <div style="width:350px">Objective</div> | <div style="width:400px">Steps</div> | <div style="width:350px">Expected Result</div> | <div style="width:50px">Type</div> |
|---|---|---|---|---|
| [PR367-TC-01](../../pallets/midnight/src/tests.rs#L194) | Verify transaction calling non-existent contract is rejected at `pre_dispatch` | 1. Initialize ledger WITHOUT deploying contract  <br>2. Create STORE_TX call (requires deployed contract)  <br>3. Call `pre_dispatch` with the transaction  <br>4. Verify rejection | `pre_dispatch` returns error; transaction NOT included in block; zero blockspace consumed | Unit |
| [PR367-TC-02](../../pallets/midnight/src/tests.rs#L235) | Verify replayed transaction is rejected at `pre_dispatch` | 1. Deploy contract via DEPLOY_TX  <br>2. Apply STORE_TX successfully  <br>3. Attempt same STORE_TX again via `pre_dispatch`  <br>4. Verify rejection | First submission succeeds; second fails with replay protection error | Unit |
| [PR367-TC-03](../../pallets/midnight/src/tests.rs#L180) | Verify valid transactions are not affected by new validation | 1. Initialize ledger state  <br>2. Call `pre_dispatch` with valid DEPLOY_TX  <br>3. Verify it passes | `pre_dispatch` returns `Ok(())`; transaction can execute successfully | Unit |
| [PR367-TC-04](../../pallets/midnight/src/tests.rs#L268) | Verify `validate_guaranteed_execution` is read-only (success path) | 1. Record ledger state root  <br>2. Call `pre_dispatch` with valid transaction  <br>3. Record state root again  <br>4. Compare roots | State roots match; no state modifications from validation | Unit |
| [PR367-TC-05](../../pallets/midnight/src/tests.rs#L297) | Verify `validate_guaranteed_execution` is read-only (failure path) | 1. Record ledger state root  <br>2. Call `pre_dispatch` with failing transaction  <br>3. Record state root again  <br>4. Compare roots | State roots match; even failed validation is read-only | Unit |
| [PR367-TC-06](../../tests/e2e/tests/lib.rs#L1100) | Verify attacker cannot fill blocks with failing transactions (single) | 1. Submit STORE_TX without prior DEPLOY_TX via RPC  <br>2. Verify RPC-level rejection | Transaction rejected at RPC level; error indicates invalid transaction | E2E |
| [PR367-TC-07](../../tests/e2e/tests/lib.rs#L1142) | Verify batch attack transactions are all rejected | 1. Submit 5 STORE_TX transactions without DEPLOY_TX  <br>2. Count rejections | All 5 transactions rejected; 0 blockspace consumed | E2E |
| [PR367-TC-08](../../tests/e2e/tests/lib.rs#L1187)** | Verify valid transactions succeed via RPC (no regression) | 1. Submit valid DEPLOY_TX via RPC  <br>2. Verify acceptance | Transaction accepted and included in block | E2E |

> [!NOTE]
> **\*\*** Tests marked with ** are temporarily ignored pending fresh node state requirement. Run manually with `cargo test-e2e-local`.

---

## Running Tests

```bash
# Run all unit tests for the midnight pallet
cargo test -p pallet-midnight --lib

# Run specific pre_dispatch tests
cargo test -p pallet-midnight --lib pre_dispatch

# Run specific test by name
cargo test -p pallet-midnight --lib test_pre_dispatch_rejects_contract_not_present

# Run E2E tests (requires running node on ws://127.0.0.1:9933)
cargo test --test e2e_tests --no-default-features --features local -- ddos --nocapture

# Run ignored E2E test manually (requires fresh node state)
cargo test --test e2e_tests --no-default-features --features local -- valid_deploy_transaction_succeeds_via_rpc --ignored --nocapture
```
