#toolkit
# Implement indexer `BuilderContext` read methods

The indexer-backed `IndexerContext` (ledger 8 and 9) now answers `ledger_parameters`,
`latest_block_context`, `contract_state` (via `contractAction(address) { state }`) and
`resolver` / `update_resolver`, and `well_formed` returns `Ok(())` instead of panicking: the
indexer has no full ledger state, and the node validates on submit. `zswap_state` and
`backs_dust_generation` remain unimplemented.

PR: <link to PR>
Issue: https://github.com/midnightntwrk/midnight-node/issues/1186
