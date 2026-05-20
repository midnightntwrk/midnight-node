#node #runtime
# Reduce cNIGHT observation UTXO query capacity by 16x

Lower the per-transaction UTXO overestimate factor from 64 to 4 in the
mainchain-follower cNIGHT observation data source. As long as the total
`utxo_capacity` stays above the max UTXOs expected in a single transaction,
the node won't get stuck on very large transactions — and the 4× factor
is ample for that. Identified via sync profiling as a cheap win on the
Postgres round-trip volume during block import.

The over-fetch factor is consensus-affecting (it shapes the inherent
payload), so the reduction is gated on the `CNightObservationApi`
runtime API version. The trait moves to v2; node binaries use 4x
when the runtime reports v2+ and fall back to the legacy 64x against
v1 runtimes. The behaviour change therefore only takes effect at the
runtime upgrade boundary and validators cannot diverge mid-rollout.

PR: https://github.com/midnightntwrk/midnight-node/pull/1367
Issue: https://github.com/midnightntwrk/midnight-node/issues/1158
