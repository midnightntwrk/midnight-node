#node #runtime #ledger

# On-chain ledger 8->9 hardfork state migration

Lets a ledger-8 chain (e.g. `1.0.1`) runtime-upgrade in place to the current
ledger-9 runtime. A new host fn, `migrate_state_v8_to_v9_step`, runs the
`StateTranslationTable` (ported from `midnight-ledger` PR #539) to translate
the on-chain `LedgerState` from v13 to v18. It's wired in as
`pallet_midnight::migrations::v2::MigrateV1ToV2`, a `SteppedMigration` under the
already-deployed `pallet-migrations`, which fires once when a ledger-8 chain
(pallet-midnight storage version 1) upgrades to this runtime (storage version 2);
a fresh ledger-9 genesis starts at version 2 and skips it.

The translation walks the whole state DAG, so its cost is unbounded in state
size — for mainnet-sized state it measures well under a second and completes in
the upgrade block itself, but a single-block migration that overran would make
the upgrade block take longer than a slot to execute on every importing node.
Running it as a multi-block migration turns that failure mode into graceful
multi-block progress: each step gets 75% of the per-block MBM service weight as
its ledger cost-model budget, parks the in-flight translation in the ledger
arena, and hands back the arena key as its cursor.

The ledger's translation engine is only resumable down to the granularity of its
persistent memo cache, so each state has a threshold budget below which a step
makes no net progress. Measured on synthetic states the threshold is tens of
milliseconds against a ~1.2s per-block budget, and it tracks the state's node
granularity rather than its total size. To guarantee termination regardless, the
migration doubles its per-step budget every 16 blocks it has been running (capped
at 2^20x); a normally-progressing migration finishes long before the first
doubling.

Multi-block migrations are serviced *after* a block's inherents, so for the
blocks the migration spans the ledger is paused: the post-block ledger update is
skipped, `pallet-cnight-observation` skips `process_tokens` without advancing
`NextCardanoPosition`, and the Cardano-to-Midnight bridge defers the whole
transfer batch without advancing its data checkpoint. Nothing is lost — both
inherents re-deliver the same data in a later block, and `frame_executive`
already restricts these blocks to inherents only.

The toolkit's fork-aware block replay is updated to match. It now picks each
block's ledger version from the tag embedded in the block's recorded state root
rather than from the runtime spec version, because during the migration window the
ledger-9 runtime is live while the state is still ledger-8; and it does not apply
blocks whose state root is unchanged from their parent's, since the chain's ledger
did not move in them either. `hardfork_e2e` waits for the finalized state to reach
ledger 9 before building post-fork transactions — a client that syncs to a
mid-migration head would otherwise build ledger-8 transactions, which those
inherent-only blocks could not have included anyway.

Also includes two fixes needed to support both ledger-8 and ledger-9 chains:
version-aware genesis seeding (detected via `serialize::peek_tag` instead of
hardcoding the v9 deserializer), and restoring the ledger-8
`construct_distribute_treasury_system_tx` host fn, which the `1.0.1` WASM
still imports.

PR: https://github.com/midnightntwrk/midnight-node/pull/1925
PR: https://github.com/midnightntwrk/midnight-node/pull/1962
Issue: https://github.com/midnightntwrk/midnight-node/issues/1580
