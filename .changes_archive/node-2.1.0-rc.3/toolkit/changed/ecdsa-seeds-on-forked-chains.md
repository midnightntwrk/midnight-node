#toolkit #ecdsa #hardfork
# Accept `ecdsa:` seeds on chains forked from ledger 8

The ECDSA guard checked the ledger version of the first block, so every chain with ledger-8
history refused `ecdsa:` seeds even after the hard fork to ledger 9. The guard now checks the
chain tip; a chain still on ledger 8 is refused as before.

Ledger 8 cannot hold an ECDSA NIGHT key, so before the fork such a seed replays with a watch-only
unshielded wallet at its ECDSA address, while its shielded and dust wallets replay normally. The
keys are installed at the fork, so shielded funds received before it stay spendable.


PR: https://github.com/midnightntwrk/midnight-node/pull/2181
PR: https://github.com/midnightntwrk/midnight-node/pull/2183
Issue: https://github.com/midnightntwrk/midnight-node/issues/2180
