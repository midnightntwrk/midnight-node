#runtime
# Accept duplicate members in Federated Authority observation

Previously, the federated authority observation pallet would reject and skip processing when duplicate Midnight members were detected in the Council or Technical Committee. This caused the node to fail to update governance membership when duplicates existed on-chain.

Now, the pallet logs an error but continues processing, allowing the node to accept governance updates even when duplicate members are present.

PR: https://github.com/midnightntwrk/midnight-node/pull/606
Ticket: https://shielded.atlassian.net/browse/PM-20680?focusedCommentId=58225
