#node
# Log an error when the D-parameter is below the permissioned candidate count

When `num_permissioned_candidates` in the D-parameter is less than the number of
permissioned candidates registered on Cardano, no candidate has a guaranteed
committee seat — risking liveness in a federated network. The node now logs an
error in this case at startup and again on every session change.

PR: https://github.com/midnightntwrk/midnight-node/pull/1506
Issue: https://github.com/midnightntwrk/midnight-node/issues/1505
