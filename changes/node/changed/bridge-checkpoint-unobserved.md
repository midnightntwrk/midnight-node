#node #bridge

# Keep a bridge data checkpoint that is ahead of the observed Cardano block

When the bridge data checkpoint is ahead of the Cardano block used for observation, the Db-Sync
bridge data source now returns it unchanged with no transfers. Previously it replaced the
checkpoint with the observed block, which moved it backwards and observed the transfers it had
already passed a second time.

PR: https://github.com/midnightntwrk/midnight-node/pull/2369
