#toolkit
# Fix panic if the first block doesn't have any midnight transactions.
It scans all blocks instead of the first one. For an RPC source it queries get_network_id API.

PR: https://github.com/midnightntwrk/midnight-node/pull/1045
Ticket: https://shielded.atlassian.net/browse/PM-22361
