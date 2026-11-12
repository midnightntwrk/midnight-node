#toolkit
# Add batch-single-tx command for bulk transaction generation

New `batch-single-tx` subcommand that generates multiple transactions from a JSON specification file. Supports per-transfer output files and configurable concurrency, with parallel ZK proving via `tokio::task::spawn_blocking`.

PR:
- https://github.com/midnightntwrk/midnight-node/pull/820
- https://github.com/midnightntwrk/midnight-node/pull/939
JIRA: https://shielded.atlassian.net/browse/PM-22103
