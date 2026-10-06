#security #rpc #ledger
# Bound ledger query APIs with the on-chain transaction byte limit

`get_decoded_transaction` and `get_transaction_cost` (reachable via `state_call`
and mempool admission) now reject transactions above the on-chain
`transaction_byte_limit` before deserialization, and decoding caps materialized
operations/identifiers at the new `MaxDecodedTxItems` pallet constant (4096).
Historic-block queries (old runtimes, v1 host functions) are bounded by the
hardcoded ledger defaults, which equal the on-chain limits at every historic
block.

PR: https://github.com/shieldedtech/shadow-midnight-node/pull/53

The transaction-pool admission path is bounded by the same on-chain limit.
`validate_transaction` and `validate_guaranteed_execution` load the ledger before
deserializing, so the limit applies there too - that path is always on, runs once per
submitted extrinsic ahead of any fee, and carries most of the cost.

A second cap bounds the DAG node count a transaction declares, at 1,024. The count is
read from the SCALE-compact length prefix following the tag, so the check costs a bounded
scan and at most five bytes rather than a parse.

Both caps come from real transactions. Over 47k mainnet transactions (2026-10-02..06) the
largest declared 333 nodes, 8 identifiers and 2 contract actions, and none exceeded 41 KB.
1,024 nodes is 3x that maximum, and 4096 decoded items stays above anything the node cap
admits.
