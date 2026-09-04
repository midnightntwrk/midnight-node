#toolkit #bugfix

# Don't abort context replay on a `well_formed` failure the chain itself tolerated

`LedgerContext::update_from_tx_with_strictness` hard-failed the whole replay
(`LedgerContextError::InvalidTransaction`) whenever a transaction's `well_formed`
check returned `Err`, e.g. `OutOfDustValidityWindow` for a dust action whose
`ctime` lands a couple of seconds past the including block's `tblock`. This made
every toolkit workflow that replays a context (transaction generation, wallet
inspection, faucets, test-data generators) unusable against a chain — such as
Preview — carrying a transaction the chain itself had accepted: on-chain,
`pallet_midnight::send_mn_transaction` hits this same check via
`LedgerApi::apply_transaction`, but only fails that one extrinsic's dispatch
(storage rolled back, `ExtrinsicFailed` emitted) without affecting block
validity, so a `well_formed` failure alone is not evidence of an invalid block.

`update_from_tx_with_strictness` now mirrors that on-chain behaviour: a
`well_formed` error is logged and the transaction is treated like a failed
apply (ledger state unchanged, no events, zero cost) instead of aborting the
replay.

PR: https://github.com/midnightntwrk/midnight-node/pull/2098
Issue: https://github.com/midnightntwrk/midnight-node/issues/2070
