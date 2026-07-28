#toolkit #performance
# Faster, resilient chain sync and wallet replay with working caching on mainnet

Fetch pipeline:
- Multi-threaded tokio runtime (previously all fetch/compute workers shared one core).
- Fetch workers reconnect with backoff instead of failing the whole sync on a
  dropped WebSocket; events and headers are fetched alongside blocks so the
  compute stage does no network I/O (and no longer panics on RPC errors).
- Events RPC is skipped for timestamp-only blocks; the state root reuses the
  existing at-block handle.
- The job pusher chases the finalized tip, so a sync ends at the current head.
- 10s progress heartbeat with rate, ETA (RPC-rate based) and backlog; startup
  and cache decisions are logged instead of silent.

Replay:
- Historical transactions are verified in proof-erased form: proof/signature
  re-verification is skipped, while per-block state-root verification against
  the on-chain `Midnight.StateKey` still guarantees correctness. ~3.5x faster
  through transaction-dense ranges.
- Partially-failed historical transactions log at debug instead of printing to
  stdout per transaction.

Wallet-state cache:
- Ledger snapshots are version-tagged and ledger-8 chains (mainnet today) are
  fully supported; previously the cache silently never saved below ledger 9,
  so every run replayed from genesis. A warm rerun now takes seconds.
- Mid-replay checkpoints (default 5 min, `MN_REPLAY_CHECKPOINT_SECS`) make
  interrupted replays resume from the last checkpoint.

PR: <link to PR>
Issue: https://github.com/midnightntwrk/midnight-node/issues/1937
