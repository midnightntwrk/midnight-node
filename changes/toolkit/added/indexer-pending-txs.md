#toolkit #indexer
# Support `generate-txs batches` with `--indexer-url`

`batches` builds transactions that spend outputs of transactions built earlier in the same run.
The new `BuilderContext::apply_pending_tx` applies each built, not yet submitted transaction to the
context's wallets, so the next one neither reuses its inputs nor misses its outputs. The replay
context delegates to `update_from_tx`, as before. The indexer context updates only its synced
wallet state: shielded coins spent and received, and unshielded UTXOs spent and created. Before
the first shielded update it brings every wallet's zswap tree up to the chain tip with one
`zswapMerkleTreeCollapsedUpdate` query per distinct start index. DUST spends were already marked
while the transaction was built.

The wallet cache still stores only indexer-confirmed state. It is captured during sync, before any
transaction is built.

`send` still fails with `--indexer-url`.

PR: <link to PR>
Issue: https://github.com/midnightntwrk/midnight-node/issues/1186
