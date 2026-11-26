#node #ledger #storage

# Add an opt-in snapshot mode for ledger storage

With `ledger_storage_mode = "snapshot"` the node writes the ledger to disk only
at snapshots of a recent finalized block, and keeps newer states in memory,
instead of persisting every block's ledger state forever. No ledger writes happen per block;
a snapshot is taken every `ledger_snapshot_interval_secs` or
`ledger_snapshot_block_cap` finalized blocks, and a restart re-executes the blocks
above the snapshot before networking starts. `StateKey`, the runtime and the host
functions are unchanged, and the default mode (`persistent`) is unaffected.

Snapshot mode needs `storage_separation = "separate"`, a non-zero
`storage_cache_size` and `--state-pruning` of at least
`2 * ledger_snapshot_block_cap + 4096`. Ledger queries of states older than
`ledger_query_window` finalized blocks fail. See `docs/ledger-snapshot-mode.md`.

PR:
