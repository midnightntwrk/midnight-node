#toolkit #ecdsa #hardfork
# Accept `ecdsa:` seeds on chains forked from ledger 8

The ECDSA guard checked the ledger version of the first block, so every chain with ledger-8
history refused `ecdsa:` seeds even after the hard fork to ledger 9. The guard now checks the
chain tip.

Ledger 8 cannot represent an ECDSA NIGHT key, so such a seed replays the pre-fork blocks under
its Schnorr identity, and only its unshielded identity is re-keyed to ECDSA at the fork block.
The shielded address is scheme-independent — `ecdsa:<seed>` and `<seed>` share one — so shielded
funds received before the fork stay visible and spendable after it.

A chain still on ledger 8 keeps the clear refusal.

PR: https://github.com/midnightntwrk/midnight-node/pull/2181
Issue: https://github.com/midnightntwrk/midnight-node/issues/2180
