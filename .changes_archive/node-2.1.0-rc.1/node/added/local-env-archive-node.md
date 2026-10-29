#local-env

# Non-validator archive node in the local-env stack

Adds `midnight-node-6` to the local-env compose: a non-validator full node
running with `--state-pruning=archive --blocks-pruning=archive`, so historical
state and block bodies stay queryable over RPC for the whole chain history. It
boots from the shared chain-spec, syncs from `midnight-node-1` as bootnode, and
exposes host ports 30338 (p2p), 9945 (RPC), and 9620 (Prometheus).

The service is labeled `io.midnight.role: archive` rather than `validator`, so
`verify-finality` and the upgrade/consensus commands — which discover
validators by that label — skip it.

PR: https://github.com/midnightntwrk/midnight-node/pull/2058
