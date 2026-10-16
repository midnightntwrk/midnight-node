#runtime #migrations

# Add runtime-upgrade migrations for Session, Bridge scripts and Babe

Dry-running the next upgrade against live `preview`, `preprod` and `mainnet`
state surfaced three gaps that would otherwise be carried through it silently:

- `Session` is still at storage version 0 after the swap to stock `pallet_session`,
  while the pallet declares 1. Wired upstream
  `pallet_session::migrations::v1::MigrateV0ToV1` — a pure version bump here, since
  `DisabledValidators` is unset on every network.
- `Bridge::MainChainScriptsConfiguration` still holds the pre-#1513 3-field value,
  which the new `MainChainScripts` cannot decode. Because the item is `OptionQuery`
  that fails silently, leaving the bridge looking unconfigured. The migration
  re-encodes it with `reserve_validator_address` empty; the real address is then set
  via `set_main_chain_scripts`.
- `Babe::EpochConfig` is unset, because pallet-babe arrived by upgrade rather than at
  genesis. That fails babe's `try_state` and would panic the pallet on the AURA→BABE
  flip. Initialized to `BABE_GENESIS_EPOCH_CONFIG`.

`pallet_session::historical` needs no migration: its prefix holds no keys on any
live network, so `BeforeAllRuntimeMigrations` initializes its version.

Also enables the `try-runtime` feature for the runtime pallets that provide one but
had it off, so their pre/post-upgrade and `try_state` hooks run in a dry-run.

PR: https://github.com/midnightntwrk/midnight-node/pull/1523
Issue: https://github.com/midnightntwrk/midnight-node/issues/2128
