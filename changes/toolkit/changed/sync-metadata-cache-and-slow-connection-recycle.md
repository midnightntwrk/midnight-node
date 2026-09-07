#toolkit #performance
# Cache runtime metadata across blocks and recycle throttled fetch connections

Found while syncing a public network against its remote RPC endpoint.

- The subxt client config now caches runtime metadata per spec version, shared by
  every client in the process. It had no cache, so subxt downloaded the full
  metadata (~130KB, two runtime calls) for every block; against a remote node fetch
  ran at ~30 blocks/s with the process idle. With the cache the same sync runs at
  ~600 blocks/s with 8 fetch workers.
- Fetch workers recycle a connection that has gone slow. Public RPC endpoints were
  observed to throttle a long-lived WebSocket after 10-20 minutes at full speed
  (25x drop, no error); a job running at under a quarter of the worker's best rate
  now triggers a reconnect, which restores full speed.

PR:
Issue: https://github.com/midnightntwrk/midnight-node/issues/1937
