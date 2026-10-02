#node
# Cache repeated mainchain-follower inherent reads during node sync

Reduce repeated db-sync round trips during node sync by caching inherent data
that is repeatedly requested for the same Cardano context. This branch caches
candidate epoch nonces per epoch and caches federated-authority observation
results per Cardano block hash so repeated council and technical committee
lookups can be served from memory.

These changes are intentionally limited to caching behavior and do not change
cNIGHT observation query flow or candidate token UTXO SQL planning.

PR: https://github.com/midnightntwrk/midnight-node/pull/1551
Issue: https://github.com/midnightntwrk/midnight-node/issues/1531
