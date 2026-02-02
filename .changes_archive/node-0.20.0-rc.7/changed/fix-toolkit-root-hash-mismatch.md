#toolkit
# Fix ledger root-hash mismatch on system transactions

Fixes regression introduced in 6644fce causing the toolkit to apply system transactions out-of-order.

**Note:** Fetch caches will have to be removed if this bug was encountered (i.e. delete `toolkit.db`)

PR: https://github.com/midnightntwrk/midnight-node/pull/515
Tickets:
- https://shielded.atlassian.net/browse/PM-21484
- https://shielded.atlassian.net/browse/PM-21483
