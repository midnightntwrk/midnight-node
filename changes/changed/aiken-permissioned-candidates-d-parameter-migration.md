# Migrate Permissioned Candidates to Aiken contracts and D Parameter RPC endpoint

This change migrates the PC (Partners Chain) validator selection system:

1. **Permissioned Candidates contracts:** Migrate from Haskell-based to Aiken-based contracts by updating `permissioned_candidates_policy_id` in all environment config files (node-dev-01, qanet, preview, preprod).

2. **New RPC endpoint:** Add `systemParameters_getAriadneParameters` to `pallet-system-parameters-rpc`. This returns permissioned candidates from Cardano Aiken contracts with D Parameter sourced from on-chain `pallet-system-parameters` storage.

3. **RPC deprecation:** Mark `sidechain_getAriadneParameters` as deprecated (still functional). Integrators should migrate to the new endpoint.

**Breaking changes:**
- `pallet-midnight`: Renumber `set_tx_size_weight` from `call_index(2)` to `call_index(1)`. Pre-encoded transactions referencing the old call index will fail.

Ticket: [PM-20994](https://shielded.atlassian.net/browse/PM-20994)
PR: https://github.com/midnightntwrk/midnight-node/pull/378
