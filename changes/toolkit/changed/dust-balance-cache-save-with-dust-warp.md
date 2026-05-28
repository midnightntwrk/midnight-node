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

Reworks `build_fork_aware_context_cached` to separate the synthetic
dust-warp block from the replay set:

- The synthetic block (detected as the last block with `number = 0`
  alongside at least one block with `number > 0`) is excluded from
  the replay on both the cold and warm paths.
- The save step now persists the post-replay context, which carries
  the real-head block's `latest_block_context` — not the wall-clock
  warp time. Save-height computation also switches from
  `blocks.last()` to `blocks.iter().max_by_key(|b| b.number)` as a
  defence-in-depth guard.
- The synthetic block is applied in-memory only as a post-save step,
  so downstream callers in the warp-enabled run
  (`register_dust_address`, batch builders) read wall-clock-now while
  the persisted snapshot stays clean. This prevents a silent warp-leak
  where a later `dust_warp = false` run against the same
  `ledger_state_db` would restore wall-clock-warp context even though
  warping is disabled.

Behaviour is unchanged for `dust_warp = false` (the only call path
the toolkit's existing CLI exercises). Adds a regression unit test in
`serde_def/transactions.rs` pinning the `from_blocks(_, dust_warp=true,
_)` synthetic-block-at-number-zero invariant that the fix relies on,
and a `check_balance_caches_at_real_head_with_dust_warp` integration
test in `dust_balance` that drives `build_fork_aware_context_cached`
directly (renumbering the fixture's blocks so `chain_id()` resolves
and the cache path actually runs) and pins five invariants:
snapshot tagged at real head, no snapshot at height 0, in-memory
tblock warped after warm restore, persisted snapshot carries the
real-head `tblock_secs` not the warp, and the warm-restore second
call does not re-persist the warp.

PR: https://github.com/midnightntwrk/midnight-node/pull/1574
Issue: https://github.com/midnightntwrk/midnight-node/issues/1573
