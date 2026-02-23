#client #node #performance
# Add TTL to soft transaction validation cache

Added a 60-second Time-To-Live for the soft validation cache to evict stale entries on relay nodes, where transactions were lingering in the mempool indefinitely.

PR: https://github.com/midnightntwrk/midnight-node/pull/737
Rebase PR: https://github.com/midnightntwrk/midnight-node/pull/748
Ticket: https://shielded.atlassian.net/browse/PM-21787
