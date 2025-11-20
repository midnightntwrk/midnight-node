#toolkit
# Fix sync-cache conflicts after a chain reset

Sync-cache files are now addressed using the block hash of the first block.

This ensure uniqueness of the sync-cache, and will fix where the sync-cache needs to be deleted after each chain reset.

PR: https://github.com/midnightntwrk/midnight-node/pull/263
Ticket: https://shielded.atlassian.net/browse/PM-20523
