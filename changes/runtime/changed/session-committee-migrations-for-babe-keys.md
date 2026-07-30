#runtime #partner-chains

# Rework committee storage migrations for the BABE session-keys upgrade

Reshapes `frame_system::SingleBlockMigrations` for
`pallet_session_validator_management` around the change that adds the BABE key to
`opaque::SessionKeys`.

Removes `migrations::v1::LegacyToV1Migration`. That migration only runs on a chain
still at on-chain storage version 0 (pre-PC-1.6). All live networks are already at
version 1 or higher — Midnight Mainnet reports on-chain storage version 1 — so it
is a no-op everywhere. There is no supported chain in the World that needs it.

Replaces the plain `migrations::v2::V1ToV2Migration` with a combined
`migrations::authority_keys::MigrateV1ToV2AddBabeSessionKeys`. The plain v1→v2
migration initializes `QueuedCommittee` from `CurrentCommittee` assuming the
committee is already in the current `SessionKeys` shape — but on Midnight the
v1→v2 upgrade is the same upgrade that adds the BABE session key, so at v1 the
stored committee (and `pallet_session` keys) are still the legacy aura+grandpa
shape and the plain migration would fail to decode them.

The combined migration (a `VersionedMigration<1, 2>`, so it runs only at on-chain
version 1 and fresh chains that genesis at version 2 skip it):

1. translates `CurrentCommittee`/`QueuedCommittee`/`NextCommittee` and the
   `pallet_session` keys from the legacy shape to the current one (the same
   translation as the pallet's `AuthorityKeysMigration`),
2. initializes `QueuedCommittee` from the now-translated `CurrentCommittee` (the
   original `V1ToV2` behavior), and
3. sets the `AddBabeSessionKeysMigrated` guard the committee decoders read to
   select the current `SessionKeys` shape.

Requires a metadata rebuild.
