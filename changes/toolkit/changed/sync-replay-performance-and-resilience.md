#toolkit #performance
# Faster, resilient chain sync and wallet replay

Backport of the toolkit sync/replay performance work from `main` (#1938) onto the
1.0.3 (ledger-8) release line, so the 1.0.3 toolkit syncs mainnet at the same speed.

Fetch pipeline:
- Multi-threaded tokio runtime (previously all fetch/compute workers shared one core).
- Fetch workers reconnect (exponential backoff, up to 10 minutes per job) instead
  of failing the whole sync on a dropped WebSocket; events and headers are fetched
  alongside blocks so the compute stage does no network I/O (and no longer panics
  on RPC errors). Losing every worker is fatal only while jobs are outstanding, so
  a warm sync with nothing to fetch cannot fail on a connection limit.
- The job pusher chases the finalized tip, so a sync ends at the current head; if
  the tip cannot be re-checked the sync stops with a warning naming the reached block.
- A cache watermark ahead of the queried node's finalized height no longer
  underflows the fetch span or lowers the verified height.
- 10s progress heartbeat with rate, ETA and backlog; startup and cache decisions
  are logged instead of silent.
- Runtime metadata is cached per spec version across all node clients. subxt used
  to download the full metadata for every block because the client config had no
  cache, which made fetch bandwidth-bound (~30 blocks/s against a remote node);
  a full sync of a public network now runs at ~600 blocks/s with 8 fetch workers.
- Fetch workers recycle a connection that has gone slow. Public RPC endpoints were
  observed to throttle a long-lived WebSocket after 10-20 minutes at full speed
  (25x drop, no error); a job running at under a quarter of the worker's best rate
  now triggers a reconnect, which restores full speed.

Replay:
- Finalized history is verified in proof-erased form: zero-knowledge proofs,
  signatures (including unshielded-input signatures) and balancing are not
  re-verified, and the remaining structural checks run on the erased transaction.
  Instead the locally computed state root is compared with the on-chain
  `Midnight.StateKey` after every block and any mismatch (or an uncomputable
  root) aborts the replay. This matches the toolkit's trust model - it is a
  testing tool that trusts the node it talks to - and is ~3.5x faster through
  transaction-dense ranges.
- Partially-failed historical transactions log at debug instead of printing to
  stdout per transaction; a 30s replay heartbeat and the end-of-replay summary
  report their counts.

PR:
Issue: https://github.com/midnightntwrk/midnight-node/issues/1937
