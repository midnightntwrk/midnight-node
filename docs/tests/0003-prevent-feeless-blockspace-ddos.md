# Test Plan: Pre-Dispatch Validation of Guaranteed Transaction Part

**ADR:** [0003-prevent-feeless-blockspace-ddos](../decisions/0003-prevent-feeless-blockspace-ddos.md)
**Ticket:** [PM-20944](https://shielded.atlassian.net/browse/PM-20944)
**PR:** [#367](https://github.com/midnightntwrk/midnight-node/pull/367)

---

## Overview

This test plan validates the DDoS mitigation implemented in ADR-0003. The fix adds `validate_guaranteed_execution` to `pre_dispatch` to reject transactions whose guaranteed part would fail before they consume blockspace.

---

## Failure Vectors Under Test

All `TransactionInvalid` variants that can occur during guaranteed part execution:

| Category | Error | Test Priority |
|----------|-------|---------------|
| **Contract** | `ContractNotPresent` | 🔴 HIGH |
| | `ContractAlreadyDeployed` | 🟡 MEDIUM |
| | `VerifierKeyNotFound` | 🟡 MEDIUM |
| | `VerifierKeyAlreadyPresent` | 🟢 LOW |
| **Replay** | `ReplayCounterMismatch` | 🔴 HIGH |
| | `ReplayProtectionViolation` | 🔴 HIGH |
| **Balance** | `InsufficientClaimable` | 🔴 HIGH |
| | `BalanceCheckOutOfBounds` | 🟡 MEDIUM |
| | `RewardTooSmall` | 🟢 LOW |
| **Zswap** | `NullifierAlreadyPresent` | 🔴 HIGH |
| | `CommitmentAlreadyPresent` | 🟡 MEDIUM |
| | `UnknownMerkleRoot` | 🟡 MEDIUM |
| **Execution** | `Transcript` | 🟡 MEDIUM |
| | `EffectsMismatch` | 🟢 LOW |
| **UTXO** | `InputNotInUtxos` | 🟡 MEDIUM |
| **Dust** | `DustDoubleSpend` | 🟡 MEDIUM |
| | `DustDeregistrationNotRegistered` | 🟢 LOW |
| **Other** | `GenerationInfoAlreadyPresent` | 🟢 LOW |
| | `InvariantViolation` | 🟢 LOW |

---

## Test Cases

### TC-0003-01: ContractNotPresent Rejection

**Objective:** Verify transaction calling non-existent contract is rejected at `pre_dispatch`.

**Preconditions:**
- Fresh ledger state from genesis
- No contracts deployed

**Steps:**
1. Deserialize `STORE_TX` (requires deployed contract)
2. Initialize ledger state WITHOUT deploying contract
3. Call `pre_dispatch` with the transaction
4. Verify rejection with `InvalidTransaction` error

**Expected Result:**
- `pre_dispatch` returns `TransactionValidityError::Invalid`
- Error contains `ContractNotPresent`
- Transaction NOT included in block
- Zero blockspace consumed

**Test Location:** `pallets/midnight/src/tests.rs`

---

### TC-0003-02: ReplayProtection Rejection

**Objective:** Verify replayed transaction is rejected at `pre_dispatch`.

**Preconditions:**
- Ledger with deployed contract
- One successful transaction already applied

**Steps:**
1. Apply `DEPLOY_TX` successfully
2. Apply `STORE_TX` successfully
3. Attempt to apply same `STORE_TX` again via `pre_dispatch`
4. Verify rejection

**Expected Result:**
- First submission succeeds
- Second submission fails with `ReplayProtectionViolation`
- Replay transaction NOT included in block

---

### TC-0003-03: Valid Transaction Passes

**Objective:** Verify valid transactions are not affected by new validation.

**Preconditions:**
- Fresh ledger state

**Steps:**
1. Initialize ledger
2. Call `pre_dispatch` with valid `DEPLOY_TX`
3. Verify it passes
4. Execute transaction
5. Verify success

**Expected Result:**
- `pre_dispatch` returns `Ok(())`
- Transaction executes successfully
- Events emitted correctly

---

### TC-0003-04: Partial Success Still Works

**Objective:** Verify transactions with guaranteed success + fallible failure work correctly.

**Preconditions:**
- Ledger with appropriate state

**Steps:**
1. Craft transaction where guaranteed will succeed, fallible will fail
2. Call `pre_dispatch` - should pass (guaranteed OK)
3. Execute transaction
4. Verify `PartialSuccess` result

**Expected Result:**
- `pre_dispatch` passes
- Transaction included in block
- `TransactionResult::PartialSuccess` returned
- Fees extracted (guaranteed succeeded)
- Fallible part rolled back

---

### TC-0003-05: Validation Does Not Modify State

**Objective:** Verify `validate_guaranteed_execution` is read-only.

**Preconditions:**
- Ledger with known state

**Steps:**
1. Record ledger state hash
2. Call `validate_guaranteed_execution` (pass or fail)
3. Record ledger state hash again
4. Compare hashes

**Expected Result:**
- State hashes match
- No state modifications from validation

---

### TC-0003-06: Attack Simulation

**Objective:** Verify attacker cannot fill blocks with failing transactions.

**Preconditions:**
- Fresh ledger state
- No contracts deployed

**Steps:**
1. Create batch of 10 transactions calling non-existent contracts
2. Attempt to include all via block building flow
3. Measure transactions included
4. Measure blockspace consumed

**Expected Result:**
- All 10 transactions rejected at `pre_dispatch`
- 0 transactions in block
- 0 blockspace consumed by attack transactions

---

## Test Matrix

| Test Case | Unit Test | Integration Test | E2E Test | Manual |
|-----------|-----------|------------------|----------|--------|
| TC-0003-01 | ✅ | ➖ | ⬜ | ⬜ |
| TC-0003-02 | ⬜ | ➖ | ⬜ | ⬜ |
| TC-0003-03 | ✅ | ⬜ | ⬜ | ➖ |
| TC-0003-04 | ⬜ | ➖ | ⬜ | ➖ |
| TC-0003-05 | ⬜ | ⬜ | ➖ | ➖ |
| TC-0003-06 | ➖ | ⬜ | ⬜ | ⬜ |

Legend: ⬜ Not Started | 🔄 In Progress | ✅ Pass | ❌ Fail | ⏭️ Skipped | ➖ N/A

---

## Manual Testing Protocol

For immediate validation without fixing test infrastructure:

| Step | Action | Expected Outcome |
|------|--------|------------------|
| 1 | Start dev node with `--dev` flag | Node running locally |
| 2 | Monitor node logs | Watch for "Pre-dispatch validation failed" messages |
| 3 | Submit transaction to non-existent contract via RPC | RPC returns error |
| 4 | Check block contents | Transaction NOT included in block |
| 5 | Verify no `TxApplied` event | Event log empty for this transaction |

---

## Success Criteria

- [x] At least one failing-guaranteed vector tested (Priority: TC-0003-01) ✅
- [x] Valid transactions confirmed still working (TC-0003-03) ✅
- [x] No regression in existing functionality ✅
- [ ] Attack simulation shows 0 blockspace consumed (TC-0003-06)

---

## References

- **ADR:** [0003-prevent-feeless-blockspace-ddos](../decisions/0003-prevent-feeless-blockspace-ddos.md)
- **Implementation:** `ledger/src/versions/common/api/ledger.rs` - `validate_guaranteed_execution`
- **Integration:** `pallets/midnight/src/lib.rs` - `pre_dispatch`

