#toolkit #performance
# Faster, resilient chain sync and wallet replay with working caching on mainnet

Fetch pipeline:
- Multi-threaded tokio runtime (previously all fetch/compute workers shared one core).
- Fetch workers reconnect (exponential backoff, up to 10 minutes per job) instead
  of failing the whole sync on a dropped WebSocket; events and headers are fetched
  alongside blocks so the compute stage does no network I/O (and no longer panics
  on RPC errors). Losing every worker is fatal only while jobs are outstanding, so
  a warm sync with nothing to fetch cannot fail on a connection limit.
- The job pusher chases the finalized tip, so a sync ends at the current head; if
  the tip cannot be re-checked the sync stops with a warning naming the reached block.
- 10s progress heartbeat with rate, ETA and backlog; startup and cache decisions
  are logged instead of silent.

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
  stdout per transaction; the replay heartbeat and the end-of-replay summary
  report their counts.

Wallet-state cache:
- Ledger snapshots are version-tagged and ledger-8 chains (mainnet today) are
  fully supported; previously the cache silently never saved below ledger 9,
  so every run replayed from genesis. A warm rerun now takes seconds.
- One predicate decides whether cached entries are usable: entries saved under
  ledger 8 are only resumed on a ledger-8 chain with every requested seed cached
  at the same height; otherwise (chain moved to ledger 9, mixed heights, uncached
  seeds) they are discarded with a warning and the replay starts from genesis.
- `--replay-checkpoint-interval` checkpoints now also work on ledger-8 chains.

PR: https://github.com/midnightntwrk/midnight-node/pull/1938
Issue: https://github.com/midnightntwrk/midnight-node/issues/1937
