#node
# Mark peer info RPC methods as unsafe

The `network_peerReputations`, `network_peerReputation`, and `network_unbanPeer`
RPC methods now require `--rpc-methods unsafe` to be called. This prevents
exposing peer reputation data and peer management on public-facing RPC endpoints.

PR: https://github.com/midnightntwrk/midnight-node/pull/1027
