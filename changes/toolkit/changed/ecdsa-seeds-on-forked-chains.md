#toolkit #ecdsa #hardfork
# Accept `ecdsa:` seeds on chains forked from ledger 8

The ECDSA guard checked the ledger version of the first block, so every chain with ledger-8
history refused `ecdsa:` seeds even after the hard fork to ledger 9. The guard now checks the
chain tip, and the context builder creates ECDSA wallets at the fork block, where the ledger-9
state first exists. A chain still on ledger 8 keeps the clear refusal.

PR: https://github.com/midnightntwrk/midnight-node/pull/2181
Issue: https://github.com/midnightntwrk/midnight-node/issues/2180
