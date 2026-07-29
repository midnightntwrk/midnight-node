#node
#runtime
# Make CNight observation mock deterministic for multi-node networks

The CNight observation data source mock now generates deterministic data based on block number instead of random data. This ensures all nodes in a multi-node network produce identical inherent data, preventing block verification failures.

**Changes:**
- Replaced random UTXO generation with deterministic generation seeded by block number
- All nodes with the same block number now generate identical mock CNight observations
- Prevents "inherent data mismatch" errors in multi-node development setups

**Technical Details:**
- Deterministic hash generation based on block number and salt
- Consistent reward addresses and dust public keys across all nodes
- Maintains compatibility with single-node development mode

This fix enables running local multi-node networks in development mode without Cardano infrastructure.

PR: https://github.com/midnightntwrk/midnight-node/pull/1870
