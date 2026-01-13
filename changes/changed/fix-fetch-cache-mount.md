#toolkit
# Fix MN_FETCH_CACHE directory mounting in toolkit entrypoint

Updates the toolkit entrypoint.sh to properly mount the MN_FETCH_CACHE directory when using file-based caching (redb: prefix). This ensures the cache directory is created with correct ownership before running the toolkit.

Also removes unused MN_SYNC_CACHE handling.

PR: https://github.com/midnightntwrk/midnight-node/pull/473
