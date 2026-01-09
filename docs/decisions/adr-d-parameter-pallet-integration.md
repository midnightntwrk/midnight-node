# ADR: D Parameter Pallet Integration

#### Status: Accepted
#### Date: 2024-12-17
#### Updated: 2025-01-09
#### Deciders: TBD

## Context and Problem Statement

The D Parameter (which controls the ratio of permissioned to registered validators) is currently fetched from a Cardano smart contract. This approach has several drawbacks:

1. **Latency** - Fetching from Cardano adds latency to the validator selection process
2. **Complexity** - Requires maintaining integration with Cardano contract
3. **Emergency override risk** - The DParameterOverride mechanism (ADR-0001) was implemented as an emergency measure but is a centralization risk

The goal is to:
- Source the D Parameter from an on-chain pallet (`pallet-system-parameters`) instead of from Cardano
- Remove the emergency override mechanism as it becomes unnecessary
- Provide RPC endpoints for consumers to query D Parameter from the authoritative on-chain source

## Decision Drivers

1. **Reduced complexity** - D Parameter sourcing from Cardano adds unnecessary complexity and latency
2. **Decentralization** - Emergency DParameterOverride is a centralization risk (noted in ADR-0001)
3. **Future governance** - `pallet-system-parameters` provides on-chain governance of system parameters
4. **Testability** - Need a mockable abstraction to develop against while the real pallet is being built

## Considered Options

### Option A: Direct pallet integration (Selected)

Source the D Parameter directly from `pallet-system-parameters` on-chain storage.

- ✅ Authoritative on-chain source
- ✅ No external dependencies for D Parameter
- ✅ Governance-ready via pallet extrinsics
- ✅ Removes centralization risk of emergency override
- ❌ Requires migration plan for consumers

### Option B: Hardcode D Parameter values in runtime

Embed D Parameter values directly in the runtime code.

- ✅ Simple to implement
- ❌ Inflexible and hard to change
- ❌ Doesn't support different values per environment
- ❌ Mixes configuration with code

### Option C: Keep DParameterOverride as the mechanism

Continue using the existing emergency override mechanism as the source.

- ✅ No new code needed
- ❌ Keeps deprecated code in the codebase
- ❌ Override mechanism was designed as emergency-only
- ❌ Perpetuates the centralization risk

## Decision

Implement **Option A: Direct pallet integration** - the D Parameter is sourced directly from on-chain pallet storage rather than from Cardano contracts.

Key outcomes:
- D Parameter is sourced from on-chain storage (authoritative source)
- Emergency override mechanism removed (no longer needed)
- New RPC endpoint `systemParameters_getAriadneParameters` provides D Parameter from pallet

## Confirmation

The decision has been validated through:

1. ✅ All existing tests continue to pass
2. ✅ Authority selection works correctly with pallet-sourced D Parameter values
3. ✅ No regression in block production or finalization
4. ✅ Emergency override mechanism fully removed from codebase
5. ✅ E2E tests validate RPC endpoints

## Notes

- Supersedes the emergency override mechanism from ADR-0001
- D Parameter is now governed on-chain via `pallet-system-parameters`, initialized at genesis
- Existing RPC endpoints that source D Parameter from Cardano are deprecated; the new `systemParameters_getAriadneParameters` endpoint sources from the on-chain pallet
- Downstream consumers (Indexer, Wallet SDK, Explorer) require coordination for migration to the authoritative D Parameter source

## References

- [ADR-0001: Ariadne Selection Emergency Override](0001-ariadne-selection-emergency-override.md)
- PR #387: Add System Parameters pallet
