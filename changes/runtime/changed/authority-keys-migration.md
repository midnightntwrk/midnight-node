#runtime #partner-chains
# Add reusable AuthorityKeys storage migration for session key encoding changes

Adds a generic, idempotent `AuthorityKeysMigration` to
`pallet_session_validator_management` for upgrading committee and session key
storage when the runtime's `AuthorityKeys` / `SessionKeys` shape changes (e.g.
during the Aura→BABE migration).

The migration uses the Polkadot SDK `VersionedMigration` pattern with explicit
`FROM`/`TO` pallet storage versions so it is safe to re-run and no-ops once the
target version is reached. It preserves key→authority mappings by translating:

- `CurrentCommittee`, `QueuedCommittee`, and `NextCommittee` in
  `pallet_session_validator_management`
- `pallet_session` `NextKeys`, `QueuedKeys`, and `KeyOwner` via
  `pallet_session::Pallet::upgrade_keys`

Runtime scaffolding lives in `runtime/src/migrations.rs` (`LegacySessionKeys`,
`LegacyCommitteeMember`, `UpgradeCommitteeMember`). It is not wired into
`SingleBlockMigrations` yet — `SessionKeys` is still aura + grandpa — and should
be connected when the arm upgrade changes the key encoding.

Includes try-runtime checks and unit tests for committee membership and key-owner
preservation.

PR: https://github.com/midnightntwrk/midnight-node/pull/1802
Issue: https://github.com/midnightntwrk/midnight-node/issues/1744
