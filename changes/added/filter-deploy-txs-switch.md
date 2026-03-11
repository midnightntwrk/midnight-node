#node
# Op::Deploy and Op::Maintain filter

Runtime `--filter-deploy-txs` switch has been added along with a TransactionPool wrapper.
When the switch is used, then node transaction pool won't accepts extrinsics that contain
Midnight `Op::Deploy` or `Op::Maintain` operations.

PR:  https://github.com/midnightntwrk/midnight-node/pull/894
JIRA: https://shielded.atlassian.net/browse/PM-22280
