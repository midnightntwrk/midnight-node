#ledger

# Add UTXO ordering overrides for historical blocks

Provide per-network UTXO ordering override data for qanet, preview, and preprod so that nodes can sync past blocks that were originally produced with non-deterministic UTXO consumption order. Includes tooling to generate the override data from an indexer database.

PR: https://github.com/midnightntwrk/midnight-node/pull/716
Related ticket: https://shielded.atlassian.net/browse/PM-21823
