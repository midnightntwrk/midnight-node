#runtime

# Enter safe mode instead of freezing the chain on a failed multi-block migration

A failed multi-block migration previously froze the chain permanently: the
migration cursor became `Stuck`, only inherents were admitted into blocks, and
`can_set_code` rejected runtime upgrades — with no on-chain recovery path on a
standalone chain.

Adds `pallet-safe-mode` (index 20) and a custom `FailedMigrationHandler` that
enters safe mode indefinitely and force-unstucks the cursor instead. The chain
keeps producing blocks with user-facing calls filtered (only inherents and the
governance allowlist pass), so governance can ship a fixed runtime and
`force_exit` safe mode. Until a migration actually fails, behavior is
unchanged.

Requires a metadata rebuild.

PR: https://github.com/midnightntwrk/midnight-node/pull/2079
