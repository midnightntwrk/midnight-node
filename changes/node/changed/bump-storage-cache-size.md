#node
# Increase default storage_cache_size to 200000

Bumped the default `storage_cache_size` in `res/cfg/default.toml` from 100000 to
200000 storage nodes to improve cache hit rates at the cost of higher memory use.

PR: https://github.com/midnightntwrk/midnight-node/pull/1729
