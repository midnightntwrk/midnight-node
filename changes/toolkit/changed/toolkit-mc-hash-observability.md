#node
# Improve McHash data source observability

Return richer outcomes for Cardano block lookups so callers can distinguish valid stable blocks from stale Cardano data, missing hashes, unstable blocks, database errors, and timestamp range failures.

This lets the node hold operations when local Cardano observability is not trustworthy, while still rejecting hashes that are missing or not stable once the Cardano tip is recent enough.

PR: https://github.com/midnightntwrk/midnight-node/pull/1552
Issue: https://github.com/midnightntwrk/midnight-node/issues/1391
