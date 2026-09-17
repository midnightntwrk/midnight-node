#runtime #migration #committee-selection

# Merge the committee v1-to-v2 migration with the BABE session-key migration

`SingleBlockMigrations` runs one custom migration, `MigrateV1ToV2AddBabeSessionKeys`
(`runtime/src/migrations.rs`), in place of
`pallet_session_validator_management::migrations::v2::V1ToV2Migration`. The partner-chains
migration cannot be combined with a `SessionKeys` shape change: it reads the committee storages
typed as the *current* `CommitteeMember`, which would decode still-legacy bytes as the new shape
and silently come back empty. Both steps therefore happen in one `VersionedMigration` (storage
version 1 → 2) that:

- translates `CurrentCommittee` and `NextCommittee` from the pre-upgrade
  `SessionKeys` (aura + grandpa) to the new shape before anything reads them with the new type,
  deriving each member's BABE key from its AURA key;
- seeds `QueuedCommittee` (added in v2) from the just-translated `CurrentCommittee`, rather than
  translating bytes a v1 chain never wrote;
- upgrades `pallet_session`'s `NextKeys`, `QueuedKeys` and `KeyOwner` through
  `pallet_session::Pallet::upgrade_keys`, counting `NextKeys` entries via `iter_keys` so the
  weight is right even while the on-chain bytes still have the old shape.

`try-runtime` `pre_upgrade`/`post_upgrade` read the legacy bytes through `unhashed` and check that
committee membership and epochs are preserved, that `QueuedCommittee` matches `CurrentCommittee`,
that every validator's keys and `KeyOwner` entries (BABE included) are upgraded in place, and that
the pallet ends at storage version 2.

The reusable `AuthorityKeysMigration` scaffolding in `runtime/src/migrations.rs`
(`LegacySessionKeys`, `LegacyCommitteeMember`, the wirability assertion) is replaced by this
migration; the generic migration itself stays in `pallet-session-validator-management` for future
key-shape changes.

Issue: https://github.com/midnightntwrk/midnight-node/issues/1742
PR: https://github.com/midnightntwrk/midnight-node/pull/2113
