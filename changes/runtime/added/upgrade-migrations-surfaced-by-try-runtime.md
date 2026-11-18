#runtime #migrations

# Add runtime-upgrade migrations for Session and Bridge scripts

Dry-running the upgrade against live `preview`, `preprod` and `mainnet` state surfaced two gaps:

- `Session` is at storage version 0 while the pallet declares 1. Wires upstream
  `pallet_session::migrations::v1::MigrateV0ToV1`, a pure version bump, as `DisabledValidators`
  is empty on every network.
- `Bridge::MainChainScriptsConfiguration` holds the pre-#1513 3-field value, which no longer
  decodes, silently, leaving the bridge looking unconfigured. It is re-encoded with an empty
  `reserve_validator_address`, to be set via `set_main_chain_scripts`.

Also enables `try-runtime` for the runtime pallets that had it off. Babe's `try_state` is
skipped until the AURA→BABE flip, as consensus-engine's `GenesisSlot` sentinel cannot pass it.

PR: https://github.com/midnightntwrk/midnight-node/pull/1523
Issue: https://github.com/midnightntwrk/midnight-node/issues/2128
