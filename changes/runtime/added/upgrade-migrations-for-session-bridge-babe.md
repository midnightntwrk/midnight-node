#runtime #migrations

# Add missing runtime-upgrade migrations for Session, Historical, Bridge scripts and Babe

The `try-runtime` upgrade dry-run against live networks surfaced four gaps that
would be carried silently through the next runtime upgrade:

- `Session`: the swap from `pallet-partner-chains-session` to stock
  `pallet_session` left the on-chain storage version at 0 while the in-code
  version is 1. Wired upstream `pallet_session::migrations::v1::MigrateV0ToV1`
  (a pure version bump on Midnight networks, where `DisabledValidators` is empty).
- `Historical`: `pallet_session::historical` is new to the runtime and had no
  storage-version initialization. Added a no-op `VersionedMigration` 0→1.
- `Bridge::MainChainScriptsConfiguration`: #1513 added `reserve_validator_address`
  to `MainChainScripts` without a migration, so values written by earlier runtimes
  fail to decode. Added a migration re-encoding the legacy 3-field value with the
  new field defaulted to the empty address (the real address must be set via
  `set_main_chain_scripts`).
- `Babe::EpochConfig`: pallet-babe added via upgrade (not genesis) leaves
  `EpochConfig` unset, failing `try_state` and poised to panic if BABE activates.
  Initialize it to `BABE_GENESIS_EPOCH_CONFIG`.

Also enabled the `try-runtime` feature for all runtime pallets that provide it
(midnight-system, preimage, session, session-validator-management,
system-parameters, version), so their pre/post-upgrade and `try_state` hooks run
in dry-runs.

PR: https://github.com/midnightntwrk/midnight-node/pull/1523
