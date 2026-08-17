#node
# Revert the cNIGHT observation sliding-window cache

Reverts "Sync Perf Part 2" (#1436). The in-memory sliding-window cache over
the four cNIGHT observation queries (registrations, deregistrations, asset
creates, asset spends) is removed; the follower goes back to the per-call
db-sync path. `cnight_follower_genesis_from_storage` and the
`cnight_observation_window_size` config key go with it.

Two independent levers from the same PR are deliberately *not* reverted: the
autovacuum tuning on the db-sync hot tables (#1434, which landed separately
anyway), and the default `storage_cache_size` of 100 000 — that one tunes the
midnight-ledger storage cache and is unrelated to the cNIGHT query path.

PR: <link to PR>
Issue: https://github.com/midnightntwrk/midnight-node/issues/1158
