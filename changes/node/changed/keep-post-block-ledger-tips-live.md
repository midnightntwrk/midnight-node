#storage #performance
# Keep post-block ledger tips live between blocks

Each post-block ledger state, and the genesis state, is held in a small LRU
cache, so the next block's first read skips re-deserializing it. The cache
holds 16 tips, enough for the fork siblings BABE produces. The tips are
persisted, so evicting one only costs a re-materialisation.

Adds `cache_type="anchored"` (`anchored_ledger_8` for ledger 8) to the
`ledger_state_cache_size` gauge.

Issue: https://github.com/midnightntwrk/midnight-node/issues/1442
