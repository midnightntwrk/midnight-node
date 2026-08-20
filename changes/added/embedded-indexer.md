#indexer #api
# Run the indexer inside dedicated API nodes

`midnight-node` can now start the wallet-facing indexer and GraphQL API in the
same process with `--indexer`. The mode is disabled by default and is intended
for dedicated API nodes because it retains the current ledger replay and SQLite
indexing workload. Indexer failures leave the node available while a supervisor
restarts the indexing pipeline; the wallet API stays unready until recovery.
The indexer source is maintained directly in the node repository rather than as
a Git submodule.

PR: https://github.com/midnightntwrk/midnight-node/pull/2051
