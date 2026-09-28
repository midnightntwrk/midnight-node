#toolkit #bugfix
# Don't abort context replay on a `well_formed` failure the chain itself tolerated

`LedgerContext::apply_txs_collect_events` (in both the ledger-8 and ledger-9
implementations, behind `update_from_block`) aborted the whole block replay
whenever a transaction's `well_formed` check returned `Err`, e.g.
`OutOfDustValidityWindow` for a dust action whose `ctime` lands a couple of
seconds past the including block's `tblock`. This made every toolkit workflow
that replays a context (transaction generation, wallet inspection, faucets,
test-data generators) unusable against a chain — such as Preview — carrying a
transaction the chain itself had accepted: on-chain,
`pallet_midnight::send_mn_transaction` hits this same check via
`LedgerApi::apply_transaction`, but only fails that one extrinsic's dispatch
(storage rolled back, `ExtrinsicFailed` emitted) without affecting block
validity, so a `well_formed` failure alone is not evidence of an invalid block.

`apply_txs_collect_events` now matches on `LedgerContextError::InvalidTransaction`
specifically and, on that variant, logs a warning and moves on to the next
transaction instead of propagating the error and aborting the replay. Every
other error variant still aborts. `update_from_tx` (used by the `batches`
generator to validate each transaction it builds before chaining more on top
of it) is untouched and keeps hard-failing: a generator needs to know
immediately if its own just-built transaction is doomed, since tolerating it
there would let a doomed transaction ship in the output batch and further
batches get built against funds it never actually created.

PR: https://github.com/midnightntwrk/midnight-node/pull/2098
Issue: https://github.com/midnightntwrk/midnight-node/issues/2070
