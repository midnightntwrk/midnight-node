# Aiken Permissioned Candidates & D Parameter Migration

#### status: proposed
#### date: 2024-12-17
#### deciders: TBD

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

* Haskell smart contracts are being deprecated in favor of Aiken contracts
* D Parameter sourcing from Cardano adds unnecessary complexity and latency
* Emergency DParameterOverride is a centralization risk (noted in ADR-0001)
* `pallet-system-parameters` is being developed to provide on-chain governance of system parameters
* Need a mockable abstraction to develop against while the real pallet is being built

## Considered Options

* **Option A:** Trait-based abstraction with mockable provider
* **Option B:** Hardcode D Parameter values in runtime
* **Option C:** Keep DParameterOverride as the mock mechanism

## Decision Outcome

Chosen option: **"Trait-based abstraction with mockable provider"**, because it provides a clean separation of concerns, follows existing codebase patterns, and allows the mock to be easily replaced with the real `pallet-system-parameters` implementation when available.

### Implementation Approach

1. **Create `DParameterProvider` trait** - Defines how to get D Parameter values
2. **Implement `MockDParameterProvider`** - Provides placeholder values until real pallet available
3. **Update `select_authorities`** - Use provider instead of optional override
4. **Remove `DParameterOverride`** - Clean up emergency mechanism from `pallet-midnight`
5. **Update config files** - Replace Haskell policy IDs with Aiken policy IDs

### Positive Consequences

* Clean abstraction that follows existing codebase patterns
* Easy to swap mock for real implementation
* Removes centralization risk of emergency override
* Simplified validator selection flow

### Negative Consequences

* Initial mock implementation requires placeholder D Parameter values
* Trait interface may need adjustment when real pallet is integrated

## Pros and Cons of the Options

### Option A: Trait-based abstraction with mockable provider

* Good, because it follows existing patterns in the codebase
* Good, because it's mockable for testing
* Good, because it cleanly separates the D Parameter source from the selection algorithm
* Good, because the mock can be easily replaced with the real pallet
* Neutral, because it requires defining a new trait

### Option B: Hardcode D Parameter values in runtime

* Good, because it's simple to implement
* Bad, because it's inflexible and hard to change
* Bad, because it doesn't support different values per environment
* Bad, because it mixes configuration with code

### Option C: Keep DParameterOverride as the mock mechanism

* Good, because no new code is needed
* Bad, because it keeps deprecated code in the codebase
* Bad, because the override mechanism was designed as emergency-only
* Bad, because it perpetuates the centralization risk

## Technical Details

### New Aiken Permissioned Candidates Policy IDs

| Environment | Policy ID |
|-------------|-----------|
| node-dev-01 | `51f812332ccc276d1dfa9da923c2235b91a5150ff275b633a5fa1bdb` |
| qa-net | `6c327f1fe5e3b2619c62ca642892146c7326a91dc47f6006f6cdf690` |
| preview | `4057188de00d74c6679263989745309f02bf55f8806061943124489b` |
| preprod | `369ee95be4c68a2984733a8c727ecd28df3039a3e5f1e80290b08eec` |

### D Parameter Policy ID

The `d_parameter_policy_id` in configuration files will be kept but ignored, as the D Parameter will now come from `pallet-system-parameters` (or mock).

### Registered Candidates

The Registered Candidates address is kept as-is and not migrated to Aiken contract.

## Validation

Measurable outcomes:

- All existing tests continue to pass
- Authority selection works correctly with mock D Parameter values
- No regression in block production or finalization
- Emergency override mechanism fully removed from codebase

## Related Decisions

- ADR-0001: Ariadne Selection Emergency Override - This ADR supersedes the emergency override mechanism introduced there

