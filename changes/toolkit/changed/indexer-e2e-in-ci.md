#toolkit #indexer #testing
# Run the indexer e2e in CI against node 2.1.0

The toolkit's indexer backend supports exactly the node versions the indexer supports, and the
indexer may trail the node. The indexer e2e (`util/toolkit/tests/indexer_e2e.rs`) therefore pins
its own node, the exact build the pinned indexer was generated against
(`midnight-node:2.1.0-rc.4` with `indexer-standalone:4.4.0-rc.6-68702121`), instead of following
CI's fresh node build. Node 2.x is ledger 9, so this covers the `ledger_9` indexer path.

Every flow also runs against indexer 4.3 (`indexer-standalone:4.3.7` with `midnight-node:1.0.0`), a
ledger-8 chain, covering the `ledger_8` indexer path and the 4.3 GraphQL schema. The
`MN_RUN_INDEXER_E2E` opt-in is gone; override the images with `INDEXER_E2E_NODE_IMAGE` and
`INDEXER_STANDALONE_IMAGE`, or `INDEXER_4_3_NODE_IMAGE` and `INDEXER_4_3_IMAGE`.

PR: <link to PR>
Issue: https://github.com/midnightntwrk/midnight-node/issues/1186
