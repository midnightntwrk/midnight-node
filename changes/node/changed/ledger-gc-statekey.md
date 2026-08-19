#ledger #storage #node
# Reclaim tagged Anchored wrappers after state pruning

The node GC worker enumerates hash-tagged ledger wrappers (staged at import,
durable on the next `on_finalize` flush) and `release_tagged`s those whose
block has left the configured state-pruning window (debounced
`have_state_at == false`). There is no AuxStore reverse index: the wrapper
tag *is* the block hash, `release_tagged` is idempotent, and the decrement
is staged into the same write cache as live block work — the next
`on_finalize` flush commits it. Arena mark/sweep still runs only when the
cache is empty (its reachability is DB-based).

`ArchiveAll` keeps history wrappers (no tip reclaim) but still runs the
arena GC loop so zero-ref Transient/intermediate garbage is culled.
`ArchiveCanonical` runs at zero lag and releases only stale-fork wrappers —
canonical history stays live. Block liveness is used rather than
`have_state_at`, which on archive backends is a state-root presence probe
that a canonical sibling sharing the same state root would keep true forever.

Capture is at every executed import (including initial sync), so a pruning
window smaller than the GRANDPA justification period no longer skips tips
the way finality-time `StateKey` reads did. The watcher reads `StateKey`
through the storage-version-gated helper (same as warp ledger-sync), so
pre-v3 block tips are tagged on a historical full sync. Pre-v3 intra-block
intermediates still leak: v1 host functions persist every successor without
unpersisting, and that is not recoverable from the native node.

An existing archive DB restarted with `--state-pruning` omitted keeps its
stored mode (`StateDb::open`); that case falls back to arena-only GC.

PR: https://github.com/midnightntwrk/midnight-node/pull/1991
Issue: https://github.com/midnightntwrk/midnight-node/issues/1983
