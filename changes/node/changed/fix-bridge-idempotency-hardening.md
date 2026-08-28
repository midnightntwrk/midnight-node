#node #runtime #c2m-bridge

# Handling ledger transaction execution errors

Bridge pallet is now ready to stop processing Cardano transaction after the first of transfers
derived from it. Note: current validators don't allow transaction that ends up in two transfers.

Cardano observability is updated so, now it understands and handles data checkpoint that
points to a Cardano transaction that hasn't been fully reflected on Midnight side.

PR: https://github.com/midnightntwrk/midnight-node/pull/1980
Issue: https://github.com/midnightntwrk/midnight-node/issues/1979
