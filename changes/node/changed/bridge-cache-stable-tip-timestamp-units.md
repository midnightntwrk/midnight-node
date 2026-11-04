#node #bridge

# Bridge cache lookahead uses the Cardano block timestamp in milliseconds

The cached bridge data source looks up the latest stable Cardano block to decide how far ahead
it can prefetch. It passed the latest block timestamp in seconds to a lookup that expects
milliseconds, so the lookup found no stable block and each refill only covered the requested
range. The timestamp is now converted to milliseconds, so refills prefetch up to
`cache_lookahead` blocks and catch-up needs fewer Db-Sync queries.

A refill always covers the requested block, even when the stable block is below it, and a refill
that finds no stable block logs it at debug level.
