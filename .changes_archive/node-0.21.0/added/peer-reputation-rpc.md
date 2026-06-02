#client #node #rpc #networking
# Add `network_peerReputations` and `network_peerReputation` RPC endpoints

Exposes peer reputation and ban status via JSON-RPC, enabling debugging of peer connectivity issues without custom tooling.

- `network_peerReputations` returns all connected peers enriched with reputation score and ban status
- `network_peerReputation` returns the same info for a single peer by ID

Can help in diagnosing issues related to peer-banning e.g: https://shielded.atlassian.net/browse/PM-21710
PR: https://github.com/midnightntwrk/midnight-node/pull/649
