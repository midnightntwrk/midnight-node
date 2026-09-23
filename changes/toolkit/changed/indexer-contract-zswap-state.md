#toolkit
# Narrow `BuilderContext::zswap_state` to `contract_zswap_state(address)`

`BuilderContext::zswap_state()` returned the whole chain's zswap state, which the indexer
cannot serve. It is replaced by `contract_zswap_state(address)`, the chain state filtered to one
contract's coins. Its root and the paths of the contract's coins match the full tree, and
that is all `send-intent` needs to spend contract-owned coins. The local backend filters its own
ledger state. The indexer backend reads `block { contractZswapState(address) }` at the latest
block.

PR: <link to PR>
Issue: https://github.com/midnightntwrk/midnight-node/issues/1186
