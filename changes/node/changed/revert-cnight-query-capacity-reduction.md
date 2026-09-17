#node #runtime
# Revert the 16x reduction in cNIGHT observation UTXO over-fetch

Reverts https://github.com/midnightntwrk/midnight-node/pull/1367, restoring the
per-transaction UTXO over-fetch factor to a flat 64x in the mainchain-follower
cNIGHT observation data source and in genesis construction.

The 4x factor left too little headroom: the per-query row limit must exceed the
UTXO count of the largest single Cardano transaction in the window, or the
truncation step emits a partial transaction and validators disagree on the
inherent payload. 64x restores that margin.

`CNightObservationApi` drops back to v1 and the node-side version gate is
removed, so every binary again uses 64x unconditionally. The runtime acceptance
envelope (`UTXO_PER_TX_OVERESTIMATE = 64`) is unchanged and was never lowered,
so both the 4x and 64x fetch factors have always been accepted on-chain.
