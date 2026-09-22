#node #runtime #ledger

# On-chain ledger 8->9 hardfork state migration

Lets a ledger-8 chain (e.g. `1.0.1`) runtime-upgrade in place to the current
ledger-9 runtime. A new host fn, `migrate_state_v8_to_v9`, runs the
`StateTranslationTable` (ported from `midnight-ledger` PR #539) to translate
the on-chain `LedgerState` from v13 to v18. It's wired in as
`pallet_midnight::migrations::v2::MigrateV1ToV2`, a `VersionedMigration<1,2,..>`
that fires once when a ledger-8 chain (pallet-midnight storage version 1)
upgrades to this runtime (storage version 2); a fresh ledger-9 genesis starts
at version 2 and skips it. The migration's weight is derived from the ledger
cost model rather than a hand-tuned estimate.

Also includes two fixes needed to support both ledger-8 and ledger-9 chains:
version-aware genesis seeding (detected via `serialize::peek_tag` instead of
hardcoding the v9 deserializer), and restoring the ledger-8
`construct_distribute_treasury_system_tx` host fn, which the `1.0.1` WASM
still imports.

PR: https://github.com/midnightntwrk/midnight-node/pull/1925
Issue: https://github.com/midnightntwrk/midnight-node/issues/1580
