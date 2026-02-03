#cnight-observation #performance

# Big speedup for cNight queries (1000x for mainnet data)

Optimized PostgreSQL queries in the mainchain follower to use the `idx_block_block_no` index instead of sequential scans. This improves cNight genesis generation performance from 1m25s to 141ms when processing 10 blocks of mainnet data.

PR: https://github.com/midnightntwrk/midnight-node/pull/525
JIRA: https://shielded.atlassian.net/browse/PM-18343
JIRA: https://shielded.atlassian.net/browse/PM-16882
