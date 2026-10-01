#node #consensus

# Interpret BABE pre-runtime digests only from the runtime that introduced `pallet-consensus-engine`

The node routes each block to the AURA or BABE pipeline by the first AURA/BABE pre-runtime digest
in its header. That layout is only guaranteed once `pallet-consensus-engine` is in the runtime;
before that an author could have put a BABE pre-runtime digest ahead of the AURA one in a block
the network accepted as a plain AURA block. Read literally, such a block would be handed to the
BABE verifier and could never be synced past — and during warp or gap sync the node has neither
state nor bodies to check whether the pallet existed.

The header carries a tamper-proof answer: `pallet-version` deposits a `Consensus` digest with the
`spec_version` of the runtime that executed the block, which an author cannot forge or omit
because the runtime rejects a block whose digest differs from the one it computed.
`consensus_engine_dispatch::babe_pre_digest_is_authoritative` is `false` exactly when that
version is below `ACTIVATION_SPEC_VERSION` (`midnight-primitives-consensus-engine`, `3_000_000`,
the first runtime with the pallet), and `authoring_engine` then reports AURA whatever the
pre-runtime digests say. `pallet-version` has deposited its digest in every block since genesis,
so a block without one can only come from a future runtime that dropped that pallet, long after
activation; its layout is trusted, so the gate does not depend on `pallet-version` staying in
the runtime forever.

All of this lives in one module, `node/src/engine_digests.rs`, which is now the only place in the
node that reads AURA/BABE pre-runtime digests: `authoring_engine` (which engine authored a block),
`slot_of` (its slot, from that engine's digest), `has_authoritative_babe_pre_digest`, and
`EngineSlotExtractor`, the single partner-chains `SlotExtractor` used by both import pipelines
(replacing the separate `AuraSlotExtractor`/`BabeSlotExtractor`). The import-queue dispatch, the
parent-slot lookup for inherent data (`slot_from_predigest`), the BABE epoch-tree seeder and the
flip watcher all delegate to it. A runtime test pins `VERSION.spec_version` at or above the
activation version.

PR: TBD
Issue: TBD
