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
3. `DParameterOverride` emergency mechanism properly removed
4. Authority selection continues to work correctly
5. New Aiken policy IDs correctly configured

---

## Components Under Test

| Component | Change | Test Priority |
|-----------|--------|---------------|
| **DParameterProvider Trait** | New abstraction for D Parameter sourcing | 🔴 HIGH |
| **MockDParameterProvider** | Returns `None` to use inherent data | 🔴 HIGH |
| **FixedDParameterProvider** | Test utility for specific D values | 🟡 MEDIUM |
| **select_authorities_with_provider** | New function using provider pattern | 🔴 HIGH |
| **DParameterOverride Removal** | Storage and extrinsic removed | 🔴 HIGH |
| **Config File Updates** | New Aiken policy IDs | 🟡 MEDIUM |

---

## Test Cases

### PR378-TC-0004-01: MockDParameterProvider Returns None

**Objective:** Verify mock provider returns `None` to maintain backward compatibility with inherent data.

**Preconditions:**
- Runtime compiled with d_parameter module

**Steps:**
1. Call `MockDParameterProvider::get_d_parameter()`
2. Verify result is `None`

**Expected Result:**
- Returns `None`, indicating inherent data should be used
- No hardcoded values returned

**Success Criteria:** ✅ Mock provider correctly defers to inherent data

**Test Location:** `runtime/src/d_parameter.rs` - `mock_provider_returns_none`

---

### PR378-TC-0004-02: FixedDParameterProvider Returns Configured Values

**Objective:** Verify fixed provider returns configured D Parameter values for testing.

**Preconditions:**
- `FixedDParameterProvider<P, R>` instantiated with specific values

**Steps:**
1. Create `FixedDParameterProvider<3, 2>` type
2. Call `get_d_parameter()`
3. Verify returned values match configuration

**Expected Result:**
- Returns `Some(DParameter)` with:
  - `num_permissioned_candidates = 3`
  - `num_registered_candidates = 2`

**Success Criteria:** ✅ Fixed provider correctly returns configured values

**Test Location:** `runtime/src/d_parameter.rs` - `fixed_provider_returns_configured_values`

---

### PR378-TC-0004-03: Provider Integration with Authority Selection

**Objective:** Verify `select_authorities_with_provider` correctly uses provider values.

**Preconditions:**
- Mock validators available (alice, bob, charlie)
- Test runtime initialized

**Steps:**
1. Create authority selection inputs with D=(1, 0)
2. Call `select_authorities_with_provider::<FixedDParameterProvider<20, 2>>`
3. Verify D Parameter override is applied
4. Check selected authorities count

**Expected Result:**
- Provider overrides input D Parameter
- Authority selection uses provider values (20 permissioned, 2 registered)
- Correct number of authorities selected

**Success Criteria:** ✅ Provider integration correctly overrides D Parameter

**Test Location:** `runtime/src/lib.rs` - `check_d_parameter_provider_integration`

---

### PR378-TC-0004-04: DParameterOverride Storage Removed

**Objective:** Verify `DParameterOverride` storage no longer exists in pallet-midnight.

**Preconditions:**
- Pallet-midnight compiled

**Steps:**
1. Verify no `DParameterOverride` storage type in pallet
2. Verify compilation succeeds without storage
3. Verify no runtime access attempts to this storage

**Expected Result:**
- Storage type removed from pallet
- No compilation errors
- Runtime uses `DParameterProvider` instead

**Success Criteria:** ✅ Legacy storage completely removed

**Test Location:** Compile-time verification

---

### PR378-TC-0004-05: override_d_parameter Extrinsic Removed

**Objective:** Verify `override_d_parameter` extrinsic no longer exists.

**Preconditions:**
- Pallet-midnight compiled

**Steps:**
1. Verify no `override_d_parameter` call in pallet
2. Verify `call_index(1)` is documented as reserved/skipped
3. Verify compilation succeeds

**Expected Result:**
- Extrinsic removed from pallet calls
- Call index preserved for compatibility (skipped)
- No compilation errors

**Success Criteria:** ✅ Legacy extrinsic completely removed

**Test Location:** Compile-time verification + code review

---

### PR378-TC-0004-06: Authority Rotation Works with Mock Provider

**Objective:** Verify committee rotation continues to work correctly.

**Preconditions:**
- Test runtime with mock provider
- Initial committee configured

**Steps:**
1. Initialize test runtime
2. Advance through epoch boundaries
3. Verify committee rotation occurs
4. Check authority selection uses mock provider path

**Expected Result:**
- Committee rotation works as before
- Mock provider defers to inherent data
- No regression in existing functionality

**Success Criteria:** ✅ No regression in committee rotation

**Test Location:** `runtime/src/lib.rs` - existing committee rotation tests

---

### PR378-TC-0004-07: Config Files Have Correct Aiken Policy IDs

**Objective:** Verify all pc-chain-config.json files updated with new Aiken policy IDs.

**Preconditions:**
- Access to config files

**Steps:**
1. Check `res/node-dev-01/pc-chain-config.json`
2. Check `res/qanet/pc-chain-config.json`
3. Check `res/preview/pc-chain-config.json`
4. Check `res/preprod/pc-chain-config.json`
5. Verify each has correct Aiken policy ID

**Expected Result:**

| Environment | Expected Policy ID |
|-------------|-------------------|
| node-dev-01 | `0x51f812332ccc276d1dfa9da923c2235b91a5150ff275b633a5fa1bdb` |
| qa-net | `0x6c327f1fe5e3b2619c62ca642892146c7326a91dc47f6006f6cdf690` |
| preview | `0x4057188de00d74c6679263989745309f02bf55f8806061943124489b` |
| preprod | `0x369ee95be4c68a2984733a8c727ecd28df3039a3e5f1e80290b08eec` |

**Success Criteria:** ✅ All config files have correct Aiken policy IDs

**Test Location:** Config file verification

---

### PR378-TC-0004-08: Aura Authorities Rotation Not Affected

**Objective:** Verify Aura authority rotation continues to work.

**Preconditions:**
- Test runtime initialized

**Steps:**
1. Run existing `check_aura_authorities_rotation` test
2. Verify authorities rotate correctly
3. Verify no errors or panics

**Expected Result:**
- Test passes
- Aura authorities rotate as expected
- No regression

**Success Criteria:** ✅ Aura rotation unchanged

**Test Location:** `runtime/src/lib.rs` - `check_aura_authorities_rotation`

---

### PR378-TC-0004-09: Grandpa Authorities Rotation Not Affected

**Objective:** Verify Grandpa authority rotation continues to work.

**Preconditions:**
- Test runtime initialized

**Steps:**
1. Run existing `check_grandpa_authorities_rotation` test
2. Verify authorities rotate correctly
3. Verify no errors or panics

**Expected Result:**
- Test passes
- Grandpa authorities rotate as expected
- No regression

**Success Criteria:** ✅ Grandpa rotation unchanged

**Test Location:** `runtime/src/lib.rs` - `check_grandpa_authorities_rotation`

---

### PR378-TC-0004-10: Cross-Chain Committee Rotation Not Affected

**Objective:** Verify cross-chain committee rotation continues to work.

**Preconditions:**
- Test runtime initialized

**Steps:**
1. Run existing `check_cross_chain_committee_rotation` test
2. Verify committee rotates correctly
3. Verify no errors or panics

**Expected Result:**
- Test passes
- Cross-chain committee rotates as expected
- No regression

**Success Criteria:** ✅ Cross-chain rotation unchanged

**Test Location:** `runtime/src/lib.rs` - `check_cross_chain_committee_rotation`

---

## Test Matrix

| Test Case | Unit Test | Integration | E2E | Manual |
|-----------|-----------|-------------|-----|--------|
| PR378-TC-0004-01 | ✅ | ➖ | ➖ | ➖ |
| PR378-TC-0004-02 | ✅ | ➖ | ➖ | ➖ |
| PR378-TC-0004-03 | ✅ | ➖ | ➖ | ➖ |
| PR378-TC-0004-04 | ✅ | ➖ | ➖ | ➖ |
| PR378-TC-0004-05 | ✅ | ➖ | ➖ | ➖ |
| PR378-TC-0004-06 | ✅ | ➖ | ➖ | ➖ |
| PR378-TC-0004-07 | ➖ | ➖ | ➖ | ✅ |
| PR378-TC-0004-08 | ✅ | ➖ | ➖ | ➖ |
| PR378-TC-0004-09 | ✅ | ➖ | ➖ | ➖ |
| PR378-TC-0004-10 | ✅ | ➖ | ➖ | ➖ |

Legend: ✅ Pass | ❌ Fail | ⏭️ Skipped | ➖ N/A

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

## Manual Testing Protocol

For validation before deploying to testnets:

| Step | Action | Expected Outcome |
|------|--------|------------------|
| 1 | Start local dev node | Node starts successfully |
| 2 | Verify runtime version | Runtime includes new changes |
| 3 | Check committee at epoch boundary | Committee rotates correctly |
| 4 | Verify permissioned validators selected | Validators match configured candidates |
| 5 | Confirm D Parameter from inherent data | Logs show D Parameter from main chain |

---

## Future Test Cases (Post pallet-system-parameters Integration)

When `pallet-system-parameters` is available:

| Test Case | Description | Priority |
|-----------|-------------|----------|
| PR378-TC-0004-11 | D Parameter read from pallet storage | 🔴 HIGH |
| PR378-TC-0004-12 | D Parameter governance update | 🔴 HIGH |
| PR378-TC-0004-13 | D Parameter change takes effect next epoch | 🟡 MEDIUM |
| PR378-TC-0004-14 | Invalid D Parameter rejected | 🟡 MEDIUM |

---

## References

- **ADR:** [0004-aiken-permissioned-candidates-d-parameter-migration](../decisions/0004-aiken-permissioned-candidates-d-parameter-migration.md)
- **Implementation:** `runtime/src/d_parameter.rs` - `DParameterProvider` trait
- **Integration:** `runtime/src/lib.rs` - `select_authorities_with_provider`
- **Pallet Changes:** `pallets/midnight/src/lib.rs` - Removed `DParameterOverride`
- **Config Files:** `res/*/pc-chain-config.json` - Updated policy IDs

