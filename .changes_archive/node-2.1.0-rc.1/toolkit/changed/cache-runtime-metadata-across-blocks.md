#toolkit #performance
# Cache runtime metadata across blocks when fetching

The subxt client config now caches runtime metadata per spec version for the
lifetime of the client. The config had no cache, so subxt downloaded the full
metadata (~130KB, two runtime calls) for every block it created an at-block
handle for; against a remote node a chain fetch ran at ~30 blocks/s with the
process idle. With the cache the same fetch runs at ~600 blocks/s with 8 fetch
workers. The cache is per client, so a process talking to several chains never
mixes their metadata.

PR: https://github.com/midnightntwrk/midnight-node/pull/2111
Issue: https://github.com/midnightntwrk/midnight-node/issues/1937
