# Migrate Permissioned Candidates to Aiken contracts and D Parameter to pallet

This change migrates the PC (Partners Chain) validator selection system:

1. **Permissioned Candidates contracts:** Migrate from Haskell-based to Aiken-based contracts by updating policy IDs in all environment config files (node-dev-01, qanet, preview, preprod).

2. **D Parameter sourcing:** Replace the Cardano D Parameter contract with on-chain `pallet-system-parameters`. Introduces a `DParameterProvider` trait for abstraction, with a mock implementation for the transition period.

3. **Remove emergency override:** The `DParameterOverride` storage and `override_d_parameter` extrinsic are removed from `pallet-midnight` as they are no longer needed. The call_index(1) slot is intentionally skipped to preserve call index stability.

**Breaking changes:** The `override_d_parameter` extrinsic is removed. This was an emergency-only feature used by root to manually set D Parameter values. It is superseded by the trait-based provider abstraction.

Ticket: [PM-20994](https://shielded.atlassian.net/browse/PM-20994)
PR: https://github.com/midnightntwrk/midnight-node/pull/378

