#runtime
# Refactor throttle account usage storage migration

Refactors the throttle pallet `AccountUsage` storage migration to follow the
Polkadot SDK `UncheckedOnRuntimeUpgrade` + `VersionedMigration<0, 1, ...>`
pattern. Keeps the existing cleanup behavior for legacy values encoded as
`(bytes_used, window_start)` and adds regression coverage for that storage shape.

PR: https://github.com/midnightntwrk/midnight-node/pull/1526
Issue: https://github.com/midnightntwrk/midnight-node/issues/1527
