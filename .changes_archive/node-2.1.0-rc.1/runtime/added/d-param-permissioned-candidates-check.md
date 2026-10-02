#runtime
# Log an error on session change when the D-parameter is below the permissioned candidate count

When `num_permissioned_candidates` in the D-parameter is less than the number of
permissioned candidates registered on Cardano, no candidate has a guaranteed
committee seat — risking liveness in a federated network. The runtime now logs
an error in this case on every session change as authorities are selected.

PR: https://github.com/midnightntwrk/midnight-node/pull/1506
Issue: https://github.com/midnightntwrk/midnight-node/issues/1505
