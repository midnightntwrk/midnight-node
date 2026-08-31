#runtime #migration #committee-selection
# Combine committee v1-to-v2 and session-key migrations

Replaced the pair of `pallet-session-validator-management` migrations (the
queue-seeding `V1ToV2Migration` and the `FROM`/`TO`-generic
`AuthorityKeysMigration`) with a single `VersionedMigration<1, 2>` that
translates `CurrentCommittee` and `NextCommittee`, seeds `QueuedCommittee`
from the translated current committee, and upgrades `pallet_session` key
storage in one step. `AddBabeToSessionKeysMigration` now wraps it,
replacing the separate queue-seeding migration whose typed read of
old-shaped committee bytes seeded an empty `QueuedCommittee`.

PR: https://github.com/midnightntwrk/midnight-node/pull/1953
Issue: https://github.com/midnightntwrk/midnight-node/issues/1742
