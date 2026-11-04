#node #bridge

# Bridge cache lookahead uses the same stability rule as MC hash verification

The cached bridge data source prefetches up to `cache_lookahead` blocks past the requested
Cardano block, capped at the latest stable block. It looked that block up with the reference
block selection rule, measured from the latest Cardano block's timestamp passed in seconds where
milliseconds were expected, so it found no stable block and each refill only covered the
requested range.

The cap is now the latest block that MC hash verification would accept at the current time:
`security_parameter` blocks deep and within the stability window, without the
`block_stability_margin` used for selecting reference blocks. Refills therefore prefetch past
the reference block, and Db-Sync is queried roughly once per `block_stability_margin` Cardano
blocks instead of once per block. A refill still covers the requested block when it is above
that cap.

PR: https://github.com/midnightntwrk/midnight-node/pull/2367
