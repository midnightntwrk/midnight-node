#toolkit #ledger9
# Store circuit zkir on-chain in test contract deploys

Ledger 9 contract operations gained an IR slot; `contract_operation_new`
now takes the circuit's zkir bytes and (ledger 9+) stores them on-chain
alongside the verifier key, so deployed contracts can be re-proven or
upgraded from chain state alone. The simple-merkle-tree test contract
deploys with IR for its `store`/`check` circuits, exercising the
`max_contract_metadata_size` check with a real payload; pre-9 ledgers
ignore the new argument. Undeployed test fixtures regenerated.

PR: https://github.com/midnightntwrk/midnight-node/pull/1692
