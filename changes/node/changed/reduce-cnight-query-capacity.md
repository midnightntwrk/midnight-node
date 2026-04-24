#node
# Reduce cNIGHT observation UTXO query capacity by 16x

Lower the per-transaction UTXO overestimate factor from 64 to 4 in the
mainchain-follower cNIGHT observation data source. As long as the total
`utxo_capacity` stays above the max UTXOs expected in a single transaction,
the node won't get stuck on very large transactions — and the 4× factor
is ample for that. Identified via sync profiling as a cheap win on the
Postgres round-trip volume during block import.

PR: https://github.com/midnightntwrk/midnight-node/pull/1367
Issue: https://github.com/midnightntwrk/midnight-node/issues/1158
