#client #node #performance
# Add TimeToIdle for transaction validation cache

Added a Time-To-Idle for the transaction validation cache to prevent entries lingering in the cache indefinitely.

Also tuned the limits of the two caches.

PR: https://github.com/midnightntwrk/midnight-node/pull/659
Ticket: https://shielded.atlassian.net/browse/PM-21787
