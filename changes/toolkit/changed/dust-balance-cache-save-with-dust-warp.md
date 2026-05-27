#toolkit
# Fix dust-balance wallet/ledger snapshot saved at `block_height = 0` when `dust_warp` is enabled

`build_fork_aware_context_cached` was using `blocks.last()` to determine
the height to tag the wallet/ledger snapshot at when saving to
`ledger_state_db`. With `dust_warp = true`,
`SourceTransactions::from_blocks` appends a synthetic timestamp-only
block with `number = 0` to the end of the block list — so the save
was tagged with `block_height = 0` even though the inner state had
been replayed up to the chain head. On subsequent runs the snapshot
was reloaded, the replay started at block 0, and dust events were
re-inserted into an already-full dust generation tree, panicking at
`ledger/helpers/src/versions/common/context.rs` with "values inserted
non-linearly".

Switches the save-height computation to
`blocks.iter().max_by_key(|b| b.number)`, which picks the highest real
block regardless of where the synthetic timestamp block lands.
Behaviour is unchanged for `dust_warp = false` (the only call path
the toolkit's existing CLI exercises). Adds a regression unit test in
`serde_def/transactions.rs` pinning the `from_blocks(_, dust_warp=true,
_)` synthetic-block-at-number-zero invariant that the fix relies on,
and a `check_balance_caches_at_real_head_with_dust_warp` integration
test in `dust_balance` that runs `execute` twice against the same
`ledger_state_db` tempdir.

PR: https://github.com/midnightntwrk/midnight-node/pull/1574
Issue: https://github.com/midnightntwrk/midnight-node/issues/1573
