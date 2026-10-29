#toolkit #indexer
# Warn when the indexer has stopped following the chain

`--indexer-url` commands now log a warning when the indexer's latest block is more than five
minutes old, since balances may then be stale and built transactions rejected. An indexer stops
this way at a runtime upgrade to a node version it doesn't support; a stalled chain looks the
same, which is why it warns instead of failing. An indexer with no blocks at all now says it
may not support the node version.

PR: <link to PR>
Issue: https://github.com/midnightntwrk/midnight-node/issues/1186
