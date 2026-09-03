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

**Epoch-tree seeding on the import path.** The first BABE block failed verification with
"Could not fetch epoch at <flip block>": BABE's epoch tree is empty before the flip and was
seeded by a task watching block-import notifications, which the client does not emit for
blocks imported with a sync origin and which in any case runs asynchronously to the BABE
worker. The dispatcher now seeds the tree at the parent of the first block of every BABE
batch right before handing the batch to the BABE queue; for a held batch that is immediately
after the AURA queue reported the flip block imported. Seeding only happens when the runtime
state at that parent has flipped to BABE (so a peer cannot make the node reset the tree at an
arbitrary block) and is a no-op once the tree covers the parent. The notification-driven
bootstrap remains only in the validator's authoring supervisor, where it sequences seeding before
the BABE worker authors the first BABE block; the separate `babe-epoch-tree-bootstrap` task for
non-authorities is removed as redundant.

**Authoring hand-over while syncing.** The supervisor's flip watcher used the plain
import-notification stream, which is silent for sync-origin imports. A validator syncing across
the flip therefore kept the AURA worker until another node's block arrived at the tip, and in
each slot until then attempted an AURA proposal that the runtime rejects ("AURA pre-runtime
digest present in state 'Babe'"); had every validator been in that state, none would have
produced that block. The watcher now uses the origin-independent every-import stream
(subscribed before the initial best-block check, with a cheap header pre-filter), so the
hand-over happens at the flip block regardless of sync state; the BABE worker idles until sync
completes. Seeding's check-and-reset is now atomic under the epoch-tree lock, since the
supervisor and the import path can seed concurrently.

**Seeding no longer wipes the epoch tree on a repeat call.** The "already seeded?" check asked
the tree for an epoch covering the flip block's children at the flip block's *own* slot. That
slot is the last AURA slot, one below BABE's genesis slot, so no epoch ever matched and every
call to seed at the flip block re-ran `reset`, discarding everything BABE had recorded since —
in particular the epoch-1 descriptor announced at the first BABE block, whose authorities differ
from the flip-time `next_epoch` because a session rotation happens in between. A node whose
second seeding ran late (a validator's supervisor after the first BABE block was imported) kept
the stale epoch 1, rejected the network's first epoch-1 block with "Bad signature", and forked.
The check now queries at the first BABE slot, and seeding refuses to reset a non-empty tree that
does not cover the flip block instead of clobbering it.

PR:
Issue:
