# ADR: Aiken Permissioned Candidates & D Parameter Migration

#### Status: Proposed
#### Date: 2024-12-17
#### Deciders: TBD

## Context and Problem Statement

The current PC (Partners Chain) validator selection system relies on:

1. **Haskell-based Permissioned Candidates contracts** on Cardano - These contracts are being deprecated in favor of new Aiken-based contracts that provide the same functionality with the same datum schema.

2. **Cardano D Parameter contract** - The D Parameter (which controls the ratio of permissioned to registered validators) is currently fetched from a Cardano smart contract. This adds latency and complexity to the validator selection process.

3. **Emergency DParameterOverride mechanism** - Implemented in ADR-0001 as an emergency measure to quickly change the validator set ratio when the Cardano-based D Parameter contract couldn't be updated. This mechanism is a centralization risk and was intended as a temporary solution.

The goal is to:
- Replace the Haskell-based Permissioned Candidates contract with the Aiken-based equivalent
- Source the D Parameter from an on-chain pallet (`pallet-system-parameters`) instead of from Cardano
- Remove the emergency override mechanism as it becomes unnecessary

## Decision Drivers

1. **Contract deprecation** - Haskell smart contracts are being deprecated in favor of Aiken contracts
2. **Reduced complexity** - D Parameter sourcing from Cardano adds unnecessary complexity and latency
3. **Decentralization** - Emergency DParameterOverride is a centralization risk (noted in ADR-0001)
4. **Future governance** - `pallet-system-parameters` is being developed to provide on-chain governance of system parameters
5. **Testability** - Need a mockable abstraction to develop against while the real pallet is being built

## Considered Options

### Option A: Trait-based abstraction with mockable provider (Selected)

Abstract the D Parameter source behind a provider interface. Use a mock provider during development that can be replaced with the real pallet when available.

- ✅ Follows existing patterns in the codebase
- ✅ Mockable for testing
- ✅ Cleanly separates D Parameter source from selection algorithm
- ✅ Mock can be easily replaced with real pallet
- ❌ Requires defining a new abstraction

### Option B: Hardcode D Parameter values in runtime

Embed D Parameter values directly in the runtime code.

- ✅ Simple to implement
- ❌ Inflexible and hard to change
- ❌ Doesn't support different values per environment
- ❌ Mixes configuration with code

### Option C: Keep DParameterOverride as the mock mechanism

Continue using the existing emergency override mechanism as a stand-in for the real pallet.

- ✅ No new code needed
- ❌ Keeps deprecated code in the codebase
- ❌ Override mechanism was designed as emergency-only
- ❌ Perpetuates the centralization risk

## Decision

Implement **Option A: Trait-based abstraction with mockable provider** because it provides a clean separation of concerns, follows existing codebase patterns, and allows the mock to be easily replaced with the real `pallet-system-parameters` implementation when available.

Key constraints:
- The abstraction must support returning on-chain values or signaling that inherent data should be used
- The Registered Candidates address is kept as-is (not migrated to Aiken)

## Confirmation

The decision will be validated through:

1. All existing tests continue to pass
2. Authority selection works correctly with mock D Parameter values
3. No regression in block production or finalization
4. Emergency override mechanism fully removed from codebase

## Notes

- Supersedes the emergency override mechanism from ADR-0001
- Aiken contracts use the same datum schema as the Haskell contracts they replace

## References

- [ADR-0001: Ariadne Selection Emergency Override](0001-ariadne-selection-emergency-override.md)

