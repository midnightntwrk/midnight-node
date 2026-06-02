#node
# Bulk-read cNIGHT observation cache to speed up genesis-to-tip sync

Replace the per-query db-sync round-trip path for cNIGHT observation data
with a startup bulk read into memory. The four observation queries
(registrations, deregistrations, asset creates, asset spends) are now
issued once across `[0, highest stable Cardano block]` (~2 M events, ~40 s
on mainnet) and held in a single sorted in-memory vector served via
`partition_point` slicing. A single-flight async sliding-window refresh
extends the cache as the chain advances, falling back to the live
db-backed source for any query past the current horizon.

Combined with the existing autovacuum tune in #1434, mainnet syncs from
genesis to tip in ~3 h 19 m (~572 k blocks).

PR: https://github.com/midnightntwrk/midnight-node/pull/1436
Issue: https://github.com/midnightntwrk/midnight-node/issues/1158
