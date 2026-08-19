#ledger #storage #node
# Reclaim tagged Anchored wrappers after state pruning

The node GC worker enumerates number-tagged ledger wrappers (persisted in
`on_finalize` / genesis / warp import) and `release_tagged`s those whose
height has left the configured state-pruning window (debounced
`have_state_at == false` on the canonical hash at that number). There is
no AuxStore reverse index: the wrapper tag *is* the block number,
`release_tagged` is idempotent, and the decrement is staged into the same
write cache as live block work — the next `on_finalize` flush commits it.
Arena mark/sweep still runs only when the cache is empty (its
reachability is DB-based).

`ArchiveAll` and `ArchiveCanonical` keep history wrappers (no tip reclaim)
but still run the arena GC loop so zero-ref Transient/intermediate garbage
is culled. Number tags cannot distinguish a canonical tip from a stale
fork at the same height, so archive-canonical does not reclaim forks.

An existing archive DB restarted with `--state-pruning` omitted keeps its
stored mode (`StateDb::open`); that case falls back to arena-only GC.

PR: https://github.com/midnightntwrk/midnight-node/pull/1991
Issue: https://github.com/midnightntwrk/midnight-node/issues/1983
