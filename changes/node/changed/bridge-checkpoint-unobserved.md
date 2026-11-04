#node #bridge

# Keep a bridge data checkpoint that the observed Cardano block has not reached

The Db-Sync bridge data source now resolves the data checkpoint against the Cardano block used
for observation before reading any data. If the checkpoint is not at or before that block,
either because it is ahead of it or because its transaction is not in Db-Sync, the data source
returns the checkpoint unchanged with no transfers, instead of moving it back to the observed
block or returning an error. A checkpoint set at genesis or by governance therefore never moves
backwards, and one that cannot be resolved can be replaced by governance.

A node whose Db-Sync has not yet reached the observed block still fails and retries. Nodes agree
on the result as long as their Db-Sync holds complete Cardano history up to the observed block.

The bridge inherent data changes only while a checkpoint is ahead of the observed block or
unresolvable, so all validators should run this version before such a checkpoint is set.
