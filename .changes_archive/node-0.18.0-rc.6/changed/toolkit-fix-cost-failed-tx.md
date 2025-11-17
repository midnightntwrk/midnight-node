#toolkit
# Fix failed transactions adding to total tx cost (StateRootMismatch)

This was causing a state root mismatch between the node and the toolkit when a tx failed

Ticket: https://shielded.atlassian.net/browse/PM-19933
Likely also fixes: https://shielded.atlassian.net/browse/PM-19877
PR: https://github.com/midnightntwrk/midnight-node/pull/110
