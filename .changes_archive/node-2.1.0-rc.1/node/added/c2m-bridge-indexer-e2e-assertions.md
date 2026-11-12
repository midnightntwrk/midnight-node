#tests #c2m-bridge #indexer

# Add indexer-side assertions to c2m_bridge e2e tests (opt-in)

Adds a new `indexer` cargo feature in `tests/e2e/` and a thin
GraphQL helper (`tests/e2e/src/api/indexer.rs`) so the four
`c2m_bridge::*` e2e tests can cross-check the indexer's bridge
surface in the same run. With the feature on, each test asserts
its matching indexer row (`BridgeUserTransfer`,
`BridgeUnapprovedTransfer`, `BridgeInvalidTransfer`,
`BridgeSubminimalFlushTransfer`) and the happy-path test
additionally pins `BridgeClaimTransaction` and the recipient's
`bridgeBalance` (deposited / claimed / balance) before and after
the claim. The `balance` field is asserted to be the post-fee
outstanding claimable, not the gross `deposited - claimed`.

When the feature is off, the suite behaves exactly as before
(node-side assertions only; no `reqwest` link, no HTTP traffic,
no indexer dependency). Run with:

```bash
cargo test --test e2e_tests --no-default-features --features local,indexer \
    c2m_bridge::
```

Also bumps the `indexer` submodule pointer to pick up the
post-fee `bridgeBalance.balance` semantics on the indexer side
(the prior build returned `deposited - claimed`, which the new
happy-path assertion rejects).

Closes the "End-to-end test against a bridge-enabled chain"
item on shieldedtech/midnight-c-to-m-protocol-bridge#4 for five
of the six bridge flows (reserve transfer remains gated on the
Cardano Reserve Validator upgrade).

PR: https://github.com/midnightntwrk/midnight-node/pull/1718
Issue: https://github.com/midnightntwrk/midnight-node/issues/1714
