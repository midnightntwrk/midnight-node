#toolkit

# Speed up toolkit syncing

Batch block-number-to-hash RPC calls into a single request instead of one call per block, reducing round trips during sync. Also simplifies several function parameters across the fetcher.

PR: https://github.com/midnightntwrk/midnight-node/pull/1263
