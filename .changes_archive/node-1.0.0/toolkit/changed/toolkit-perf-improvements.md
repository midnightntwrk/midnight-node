#toolkit
# Improve toolkit block replay and transaction generation performance

Batches state-change events during block replay for wallet initialization, using biased `tokio::select!` to prioritize new work from fetch workers and reduce incremental processing overhead. Adds a `--replay-concurrency` CLI parameter (defaults to CPU core count) and uses Rayon-based parallel wallet updates during replay.

Adds structured `[perf]` logging for timing instrumentation of key operations. Includes a change from BSON encoding to postcard which cuts the size of cached blocks by half and cached ledger states by ~8 and graceful failure when the wallet has insufficient DUST balance.

PRs:
- https://github.com/midnightntwrk/midnight-node/pull/820
- https://github.com/midnightntwrk/midnight-node/pull/939
JIRA: https://shielded.atlassian.net/browse/PM-22103
