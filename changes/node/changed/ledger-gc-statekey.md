#ledger #storage #node
# Reclaim Anchored tips after Substrate state pruning

The node GC worker reads `pallet_midnight::StateKey` at finality
(`tree_route` + `stale_blocks`) and stores `(block_hash → tip)` in AuxStore
only when the tip decodes as a reclaimable ledger-8/9 arena key (pre-ledger-8
roots are not indexed and leak). Pre-v3 raw `StateKey` blobs are accepted
through the same filter. Once past the configured state-pruning lag and
debounced `have_state_at == false`, it retires the hash from AuxStore and then
`unpersist`s each tip once and flushes the ledger write cache (remove-first:
the decrement is not idempotent, so a crash between the two steps costs a
leak, never a double-decrement; without the flush, shutdown could drop
staged decrements after AuxStore retirement and leave roots unreclaimable).
The reclaim hold is quiescence-gated: it defers while the shared ledger write
cache is dirty, so the durability flush only ever writes isolated GC
decrements — flushing mid-execution would land another block's staged
unrooted nodes in the DB and let the arena sweep's quiescence check pass
while the runtime still needs them. Decrements are zero-clamped (a
crash-replayed binding across the two independent parity-db WALs, or a
state-synced block that never executed locally, is a logged no-op instead of
a root-count underflow).
Incremental arena `gc` runs once per finality after reclaim (including during
major sync), with a short soft time bound — further progress resumes on later
notifications instead of looping under the arena mutex. GC runs only while the
ledger write cache is quiescent: its DB-based mark/sweep cannot see staged
block state, so sweeping mid-execution could cull nodes the in-flight state
references; a dirty cache defers the sweep to the next slice.
`ArchiveAll` keeps Anchored history tips (no tip reclaim) but still runs the
arena GC loop so zero-ref Transient/intermediate garbage is culled.
`ArchiveCanonical` runs at zero lag, binds only stale-fork tips (canonical
bindings could never be reclaimed and would grow the AuxStore blob forever)
and reclaims them by block liveness — once finality passes the fork's height
and it is not the canonical block there — rather than `have_state_at`, which
on archive backends is a state-root presence probe that a canonical sibling
sharing the same state root would keep true forever. Canonical archive
history stays live. Fork capture at finality is best-effort: non-canonical
state is dropped in the same finalize commit, so tips unreadable by then
leak (bounded by fork rate).

Capture happens at finality, so blocks whose state is pruned in the same
batched-finalization commit (justification period > pruning window) can never
be captured and leak; the node warns at startup when
`--state-pruning < 512` — full-sync-from-genesis nodes should use a window of
at least the GRANDPA justification period.

An existing archive DB restarted with `--state-pruning` omitted keeps its
stored mode (`StateDb::open`); since the stored variant is not exposed through
the public API, that case falls back to arena-only GC (correct for
`ArchiveAll`; a stored `ArchiveCanonical` forgoes stale-fork reclaim,
leak-only) instead of mistakenly binding immortal canonical tips on the
constrained path. Pass the mode explicitly to opt into stale-fork reclaim.

PR: https://github.com/midnightntwrk/midnight-node/pull/1991
Issue: https://github.com/midnightntwrk/midnight-node/issues/1983
