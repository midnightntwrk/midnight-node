#indexer #api
# Run the indexer inside dedicated API nodes

`midnight-node` can now start the wallet-facing indexer and GraphQL API in the
same process with `--indexer`. The mode is disabled by default and is intended
for dedicated API nodes because it retains the current ledger replay and SQLite
indexing workload. If the embedded indexer exits, the node exits too so process
supervision cannot leave a false-healthy API node.

PR: https://github.com/midnightntwrk/midnight-node/pull/2051
