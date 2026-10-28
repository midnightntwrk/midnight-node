#node
# Revert the cNIGHT observation sliding-window cache

Reverts "Sync Perf Part 2" (#1436) while its security hardening is finished off.
The in-memory sliding-window cache over the four cNIGHT observation queries
(registrations, deregistrations, asset creates, asset spends) is removed and the
follower goes back to the per-call db-sync path, along with
`cnight_follower_genesis_from_storage` and the `cnight_observation_window_size`
config key.

Sync from genesis is slower again as a result — #1158 is reopened in practice
until the cache lands a second time.

Two independent levers bundled into the same PR are deliberately kept, as
neither touches the cNIGHT query path:

- the autovacuum tuning on the db-sync hot tables (#1434), which landed via its
  own PR and so was never part of this diff;
- the default `storage_cache_size` of 100000, which sizes the midnight-ledger
  storage cache.

PR: https://github.com/midnightntwrk/midnight-node/pull/2030
Issue: https://github.com/midnightntwrk/midnight-node/issues/1158
