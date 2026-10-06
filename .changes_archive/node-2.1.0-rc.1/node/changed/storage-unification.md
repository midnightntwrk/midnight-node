#storage
# New `storage_separation` config option to use a single ParityDb instance

By default, the node uses separate ParityDb instances to store Midnight Ledger and Substrate storage items. This change adds a new config option, `storage_separation` to allow node operators to store all storage items in the same instance.

Using `storage_separation=unified` reduces the likelihood of data integrity errors in the case of unexpected node process termination.

PR: https://github.com/midnightntwrk/midnight-node/pull/1278
Issue: https://github.com/midnightntwrk/midnight-node/issues/1297

