#storage #gc #performance
# Reclaim dead ledger tips with a background tagged-root GC

`ledger_storage` grew without bound on pruned nodes: every post-block ledger tip was persisted
as an arena GC root and nothing released it when Substrate pruned the block's state (issue 1983
measured 197 GB of ledger storage against a 6.8 GB paritydb).

## Mechanism

Every durable ledger tip (post-block, genesis, warp-sync import and the ledger 8 to 9 migration
output, in both ledger bridges) is persisted through a `TaggedRoot` wrapper tagged with the
parent hash of the block that produced it. The ledger database stays the only source of truth
and the emitted state key, host functions and runtime are unchanged.

A background worker runs one slice per finality notification. A slice scans the root set (the
pruning window plus forks on a pruned node), marks a wrapper dead once its block height is below
`finalized - state_pruning` and no child of its parent block still has state, confirmed on two
consecutive slices, and releases the dead wrappers. A bounded incremental arena sweep runs
after block imports, paced to a maximum share of wall time. The worker keeps no state across slices beyond that debounce, every error path keeps
the wrapper, and it keeps running during major sync so a resync from genesis stays bounded by
the pruning window.

## Operator notes

- **Resync to enable GC.** Roots written by earlier node versions carry no tag and can never be
  released, so GC does not run on a ledger storage they created. A node that creates a fresh
  ledger storage records the marker `midnight:ledger_gc_tagged_roots` in the main database's
  META column; at startup the worker reads that single key and, when it is absent, logs a resync
  message once and reports `ledger_gc_suspended{reason="legacy_roots"}`. No root is iterated to
  decide this, so the size of an old database is irrelevant. Deleting only `ledger_storage/` is
  not a resync. An upgraded-in-place node behaves exactly as before this change; nodes can be
  resynced one at a time and GC is active on every node resynced with this version.
- **Multi-block migrations.** Release is suspended while `MultiBlockMigrations::Cursor` exists
  (or cannot be read) at the finalized block. A migration may rely on any ledger root inside the
  pruning window at the upgrade block. Pallets must not keep an old ledger root in ordinary
  storage outside a multi-block migration, and migration code must persist through the tagged
  path, never raw `persist`.
- **Archive modes** (`archive`, `archive-canonical`, unbounded constrained) run no GC by
  default; logged once at startup. `--ledger-state-pruning <N>` opts such a node in: the ledger
  state of every block below `finalized - N` is released, stale forks included, while Substrate
  state stays archival. Anything that loads the ledger state of an older block on that node
  fails (runtime APIs such as `get_zswap_state_root` at that block, warp ledger-sync requests
  for it), so an indexer served by the node must validate against `pallet_midnight::StateKey`
  (midnight-indexer PR 1724 or later) before the flag is enabled. On a node with a bounded
  `--state-pruning` window the flag may only widen the ledger window; a smaller value is
  refused at startup. The marker rule above applies unchanged: the flag needs a ledger storage
  created by a node with ledger GC.
- **Unified storage** (issue 2294): storage-core cannot iterate ledger columns created without
  its btree index, which unified mode does not set yet (PR 2300 adds it). The worker probes
  once at startup and disables itself with a resync message
  (`ledger_gc_suspended{reason="unindexed_columns"}`) instead of panicking in every slice.
- **A panicking sweep disables itself** for the process
  (`ledger_gc_suspended{reason="sweep_panic"}`); the panicking batch deletes nothing and release
  continues.

## Observability

- `ledger_gc_phase_time{phase=scan|release|arena_gc|arena_gc_dirty}` (`arena_gc_dirty` counts
  sweeps refused because the next block had already written to the ledger cache)
- `ledger_gc_tagged_roots{kind=total|candidates|pinned}`
- `ledger_gc_released_total`, `ledger_gc_culled_total`
- `ledger_gc_suspended{reason=mbm|sweep_panic|legacy_roots|unindexed_columns}`
- a warning when an arena hold exceeds 50 ms (or twice the slice budget for the sweep);
  suspend and resume transitions are logged

## Tuning

- `MIDNIGHT_LEDGER_GC_SLICE_MS`: arena sweep budget per slice (default 200). On the perfnet A/B
  25 ms and 100 ms fell behind at ~10 transactions per block; 200 ms kept the ledger flat
  through a 14-hour load with no import penalty, for about 21% more disk writes and 11% more
  CPU than running no sweep. Sustained load at block capacity is untested; the hold warning
  fires above twice the budget.
- `MIDNIGHT_LEDGER_GC_MAX_DUTY_PCT`: maximum share of wall time the arena sweep may hold the
  ledger arena (default 20, range 1 to 100). Sweep slices follow block imports, including
  imports during major sync, at most one per `slice * 100 / duty`. With 6-second blocks the cap
  never binds and the node sweeps once per block; during a resync (about 6 blocks per second)
  it does, so the sweep gets 20% of wall time instead of one slice per finality notification
  (about 1% on the perfnet resync, where no mark cycle completed and the ledger kept the full
  garbage of the replayed history). Import slows by at most that share while syncing.
- `MIDNIGHT_LEDGER_GC_ARENA_SWEEP`: `0` releases tips but never sweeps; escape hatch if the
  sweep's mark set uses too much memory on a very large arena.
- `--ledger-state-pruning <N>`: explicit ledger window (see *Archive modes*).
- Per-slice work grows with the `--state-pruning` window (one read and one header lookup per
  root).

Issue: https://github.com/midnightntwrk/midnight-node/issues/1983
Issue: https://github.com/midnightntwrk/midnight-node/issues/2294
PR: https://github.com/midnightntwrk/midnight-node/pull/1991
