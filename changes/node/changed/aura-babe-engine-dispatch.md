#node #consensus #babe #aura

# Run AURA and BABE side by side: one import queue, one authoring gate

Across the AURA→BABE flip a node must be able to verify, import and author blocks of both
engines. Both pipelines are now built at start-up and dispatched per block.

**One import queue, routed by the authoring engine.** The node keeps a single `BasicQueue`.
`EngineDispatchVerifier` / `EngineDispatchBlockImport` (`node/src/consensus_engine_dispatch.rs`)
hand each block to the AURA or BABE verifier and block import by the engine that *authored* it,
read from the block's own header: the engine id of its first AURA or BABE pre-runtime digest.
Chain state cannot be the routing key — a sync batch `[…, flip, flip+1, …]` carries the first BABE
block together with its parent, so the parent's post-flip state does not exist when the batch is
queued. The header can, because `pallet-consensus-engine` asserts for every executed block that
the AURA pre-digest comes first before the flip and that no AURA pre-digest is present after
it. Digests of other engines (e.g. the main-chain hash) are skipped; a header with neither
defaults to AURA, whose verifier gives the clearer error. Routing decides *which* verifier runs,
not whether a block is valid — a block whose digests misstate its engine still fails the receiving
verifier or the pallet's own digest assertions.

Ordering at the flip needs no cross-engine coordination: the queue's single worker verifies and
imports in submission order, so the first BABE block is only verified once the flip block is in.

**BABE pipeline composition.** The BABE block import is the partner-chains sandwich
`PartnerChainsBlockImport<BabeBlockImport<PartnerChainsBodyRestore<GrandpaBlockImport>>>`: the
outer layer runs the full Partner Chains inherent check with `VerifierCIDP` and withholds the body
from BABE, so BABE's own (redundant, minimal-CIDP) inherent check is skipped while its
epoch/equivocation logic still runs; the body is restored for the GRANDPA import beneath. The BABE
verifier is wrapped the same way. The warp ledger-sync gate now wraps the dispatching block import,
so post-warp BABE blocks are held during arena recovery exactly like AURA blocks.

**Epoch-tree seeding on the verify path.** Nothing is imported through the BABE pipeline before
the flip, so `BabeLink`'s `EpochChanges` is empty and the first BABE block has no epoch to be
verified under ("Could not fetch epoch at <flip block>"). Block-import notifications cannot drive
seeding: the client emits none for sync-origin imports, and even at the tip a notification-driven
task runs asynchronously to the import worker. Instead `EngineDispatchVerifier` asks
`BabeEpochSeeder` to cover a block's parent immediately before handing the block to the BABE
verifier. Seeding is idempotent, cheap once the tree covers the parent, and refuses to seed at a
block whose state has not flipped to BABE or to clobber a non-empty tree that does not cover the
flip block — the parent comes from a peer-supplied header, so this is what stops a peer from
resetting the tree at an arbitrary block.

**One authoring gate.** The two slot workers cannot run concurrently: the idle one would spam
failed aux-data fetches every slot, and if both authored they would fork the chain.
`run_authoring_supervisor` (`node/src/babe_authoring.rs`) is the only authoring task. It polls
AURA until the flip, seeds the epoch tree, then polls BABE for the rest of the node's life; the
switch is one-directional, so a restart after the flip skips AURA. The flip watcher uses the
origin-independent every-import stream, not the plain import-notification stream, which is silent
for sync-origin imports — otherwise a validator syncing across the flip would keep the AURA worker
and attempt AURA proposals the runtime rejects ("AURA pre-runtime digest present in state 'Babe'")
until another node's block reached its tip. The check-and-reset in seeding is atomic under the
epoch-tree lock, since the supervisor and the import path can seed concurrently. Non-authorities
run no flip watcher at all: the verify-path seeder is enough to import the first BABE block.

**Starting on a runtime that has no `BabeApi`.** `BabeBlockImport` is constructed in `new_partial`
even while the chain is still on AURA, but `sc_consensus_babe::configuration` requires on-chain
`BabeApi`, so a node binary rolled out before the runtime upgrade that adds `pallet-babe` would
refuse to start. `configuration_at_startup` synthesizes a placeholder configuration from the AURA
slot duration and the sidechain epoch length in that case; real epoch descriptors are always seeded
from `BabeApi` at the flip, never from the placeholder.

**Slot extraction on both sides.** `BabeSlotExtractor` reads the slot from the BABE pre-runtime
digest for the BABE pipeline, and `slot_from_predigest` (used for the parent slot in the inherent
data providers) now tries AURA first and falls back to BABE, so a parent authored by either engine
resolves across the flip.

PR: https://github.com/midnightntwrk/midnight-node/pull/2113
Issue: https://github.com/midnightntwrk/midnight-node/issues/1757
