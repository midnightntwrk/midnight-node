#node

# Do not ban peers when local Cardano observation lags during block verification

Problem being solved: when a node's local Cardano observation (db-sync) lags,
the node is being banned by its peers.
When block verification fails due db-sync lag, node re-requests the block from peers,
and doing so leads to reputation decrease ending up with a ban.

Solution to this problem is to hold up when the verified block is not stable or unknown.
That are separate cases.
If the block reference is not found at all, node triggers await loop only if our cardano view is certainly bad according to the Praos rules.
If the block is known but misses confirmations (but its timestamp is valid), the node enters a loop awaiting a fresh Cardano block to appear before checking the block again.
If the Cardano block timestamp is out of range in relation to the substrate block timestamp, then the block is immediately rejected as invalid.
This last condition narrows possibility of referencing blocks without enough confirmations, to trigger await loop in the rest of network.

PR: https://github.com/midnightntwrk/midnight-node/pull/1472
Issue: https://github.com/midnightntwrk/midnight-node/issues/1391
