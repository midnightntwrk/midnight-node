#node #consensus #babe #aura
# Route by authoring engine and order BABE behind AURA in `DispatchImportQueue`

Two fixes to how `DispatchImportQueue` hands blocks to the AURA and BABE import queues
across the AURA→BABE flip, both of which showed up when syncing a batch that straddles the
flip block.

**Routing key.** Blocks were routed by the engine active in the *parent's* runtime state.
That state does not exist for the first BABE block in a sync batch, whose parent (the flip
block) is still in the same batch, so the query failed, fell back to AURA, and the block was
rejected by the AURA verifier. Sync then dropped the peer and restarted, advancing one BABE
block per restart. Blocks are now routed by the engine that authored them, read from the
header: the engine id of the first AURA or BABE pre-runtime digest. `pallet-consensus-engine`
guarantees this is decisive for every valid block (AURA pre-digest first while armed, none
after the flip), so routing stays keyed to the invariant the runtime enforces. Digests from
other engines (e.g. the main-chain hash) are skipped; blocks with no header or no such digest
still default to AURA.

**Ordering.** The two queues are separate `BasicQueue`s with their own worker tasks, and
submitting only enqueues on a worker's channel, so the BABE worker could verify the first BABE
block before the AURA worker had imported the flip block, both within one straddling batch and
across consecutive batches queued by sync, failing with `UnknownParent`. The dispatcher now
tracks AURA batches whose results have not yet come back through the import-queue `Link` and
holds BABE batches while any are in flight, releasing them in submission order once the AURA
queue drains. Held batches are never dropped, since sync expects a result for every block it
queued. AURA never waits for BABE, and outside the transition only one side is ever non-empty,
so the gate is inert before and after the flip.

PR:
Issue:
