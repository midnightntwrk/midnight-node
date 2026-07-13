#toolkit
# Fix toolkit waiting indefinitely when a connection fails

Previously, the toolkit fetch mechanism would wait indefinitely if a
fetch worker failed to connect to the RPC node. Now, it waits a max 15
seconds before logging a warning. If all workers fail to connect, an
error is returned.

This fix has also been applied to block fetches - fetch will now fail with an
error when block fetches are retried for more than 30 seconds.

PR: https://github.com/midnightntwrk/midnight-node/pull/405
Fixes: https://shielded.atlassian.net/browse/PM-20917
