#node
# parity-db: Midnight fork, lower flush threshold, ahash

The workspace `parity-db` dependency is updated to the Midnight fork
(`github.com/midnightntwrk/parity-db`) instead of the crates.io release. The
fork sets a smaller background flush threshold (64 → 16) and uses `ahash` for
hashing:

https://github.com/midnightntwrk/parity-db/pull/1
https://github.com/midnightntwrk/parity-db/pull/2

PR: https://github.com/midnightntwrk/midnight-node/pull/1478
Issue: https://github.com/midnightntwrk/midnight-node/issues/1485
