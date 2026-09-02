#indexer #api
# Run the indexer on dedicated API nodes

`midnight-node` can now start the wallet-facing indexer and GraphQL API with
`--indexer`. The mode is disabled by default and is intended for dedicated API
nodes because it retains the current ledger replay and SQLite indexing workload.
Indexer failures leave the node available while a supervisor restarts the native
worker; the wallet API stays unready until recovery. The indexer source and worker
are built directly in the node repository rather than from a Git submodule or a
separately released component.

PR: https://github.com/midnightntwrk/midnight-node/pull/2051
