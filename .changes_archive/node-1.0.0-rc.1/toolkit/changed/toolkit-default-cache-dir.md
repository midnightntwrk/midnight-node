#toolkit
# Change default cache location to `./toolkit_cache` instead of `./toolkit.db`

This was required because we now have two separate caches - one for the fetch cache, the other for the wallet state cache.

PR: https://github.com/midnightntwrk/midnight-node/pull/939
Ticket: https://shielded.atlassian.net/browse/PM-22103
