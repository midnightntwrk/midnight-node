#node
# Speed up mainchain-follower db-sync reads during node sync

Reduce mainchain-follower sync latency versus `release/node-1.0.0` by cutting
repeated db-sync round trips and improving slow inherent-data queries. This
branch adds caching for candidate epoch nonces and federated-authority
observations, rewrites the candidate token UTXO lookup to force an
`ma_tx_out.ident`-first plan, and parallelizes the cNIGHT and federated
authority subqueries that were previously serialized.

In the preview sync reproduction used for validation, these changes improved
steady-state sync throughput from about 1.1 blocks/sec to about 2.0 blocks/sec.

PR: https://github.com/midnightntwrk/midnight-node/pull/1546
Issue: https://github.com/midnightntwrk/midnight-node/issues/1531
