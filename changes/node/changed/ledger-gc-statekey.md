#ledger #storage #node
# Reclaim Anchored tips after Substrate state pruning

The node GC worker reads `pallet_midnight::StateKey` at finality
(`tree_route` + `stale_blocks`) and stores `(block_hash → tip)` in AuxStore.
Once past the configured state-pruning lag and debounced
`have_state_at == false`, it retires the hash from AuxStore and then
`unpersist`s each tip once (remove-first: the decrement is not idempotent, so
a crash between the two steps costs a leak, never a double-decrement).
Incremental arena `gc` runs after reclaim (deferred during major sync).
Archive backends skip the worker.

Capture happens at finality, so blocks whose state is pruned in the same
batched-finalization commit (justification period > pruning window) can never
be captured and leak; the node warns at startup when
`--state-pruning < 512` — full-sync-from-genesis nodes should use a window of
at least the GRANDPA justification period, or warp sync (skipped history is
never executed, so nothing leaks).

PR: https://github.com/midnightntwrk/midnight-node/pull/1991
Issue: https://github.com/midnightntwrk/midnight-node/issues/1983
