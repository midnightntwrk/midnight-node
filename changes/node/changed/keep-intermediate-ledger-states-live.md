#storage #performance
# Keep intra-block ledger states live instead of persisting them

Every intra-block ledger state used to be made a GC root: `apply_transaction` /
`apply_system_transaction` persisted the state they produced and unpersisted their
predecessor. The persist existed for one mechanical reason — `Sp::drop` runs when the
host function returns, and uncaching a non-persisted state removes it from the arena
entirely — so rooting it was the only thing keeping it addressable for the next call.

The `Sp` is now held in a process-global keep-alive cache instead, and released by the
successor call. One `persist()` per block (the post-block tip) replaces one persist plus
one unpersist per transaction.

The win is not fewer DB transactions — `persist`/`unpersist` were already in-memory
refcount updates, and the single parity-db commit per block is unchanged. It is fewer
deserializations. The arena's caches hold binary objects and its `sp_cache` holds only
weak references, so dropping the `Sp` tree at every host-function boundary meant every
call re-materialised the ledger working set from scratch — roughly three times per
transaction (`get_tx_weight`, `pre_dispatch`, `apply_transaction`) plus once at
`on_finalize`. Holding the live `Sp` for both the intra-block intermediates and the
post-block tip collapses that to one materialisation per block.

Two caches, with different contracts:

- **transient** (intra-block intermediates, capacity 1024, 60s TTI) — not persisted, so
  this cache is the only thing keeping them addressable. Entries are refcounted, because
  two executions can be in flight at once (authoring alongside import) and two forks off
  the same parent applying the same transaction produce the same content hash. The TTI
  bounds the leak if an execution abandons its tail.
- **anchored** (post-block tips, capacity 4, 300s TTI) — these *are* persisted, so a miss
  just costs one re-materialisation from the arena; no refcount, no release, and eviction
  is always safe.

`ledger_state_cache_size{cache_type="transient"|"anchored"}` reports both, sampled once
per block at the post-block flush. `transient` must read 0 there; a persistent non-zero
value is a leaked intra-block state.

Also fixes `has_ledger_state`, which is documented to return `false` for an unresolvable
state key but reached that branch by panic: `get_ledger` now probes the arena root before
handing back a lazy pointer that panics on first deref.

The emitted state key is unchanged: the `Ledger` root is always an `ArenaKey::Ref`, so
`Sp::persist`'s `Direct -> Ref` promotion never fired and the serialized key is
byte-identical with or without the persist. No storage-layout, host-ABI or runtime change.

Issue: https://github.com/midnightntwrk/midnight-node/issues/1442
PR: https://github.com/midnightntwrk/midnight-node/pull/2050
