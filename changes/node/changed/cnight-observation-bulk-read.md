#node
# Bulk-read cNIGHT observation cache to speed up genesis-to-tip sync

Replace the per-query db-sync round-trip path for cNIGHT observation data
with an in-memory sliding-window cache. The four observation queries
(registrations, deregistrations, asset creates, asset spends) feed a single
sorted in-memory vector served via `partition_point` slicing. The cache
starts empty: the first inherent query after startup anchors a background
refresh at the runtime's latest processed Cardano position and pulls events
from there up to the highest stable Cardano block, so a restarted node only
fetches the window it needs rather than `[genesis, tip]`. Single-flight
async refreshes slide the window forward as the chain advances, falling
back to the live db-backed source for any query outside the cached window.

Combined with the existing autovacuum tune in #1434, mainnet syncs from
genesis to tip in ~3 h 19 m (~572 k blocks).

Also raises the default `storage_cache_size` (the midnight-ledger storage
cache, in entries) from 10 000 to 100 000. This is an independent sync-perf
lever from the cNIGHT cache above: a larger ledger-state cache cuts evictions
and misses during the heavy state replay of a full sync. The tradeoff is
higher steady-state memory for that cache, which is an acceptable cost for the
sync-speed improvement.

PR: https://github.com/midnightntwrk/midnight-node/pull/1436
Issue: https://github.com/midnightntwrk/midnight-node/issues/1158
