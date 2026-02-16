#node #client
# Sync with non-determinism, produce blocks with determinism

Until we patch the chain, we need to allow nodes to sync to tip. This change preserves the non-deterministic behaviour of syncing, while ensuring new blocks are produced deterministically.

PR: https://github.com/midnightntwrk/midnight-node/pull/685
Related ticket: https://shielded.atlassian.net/browse/PM-21823
