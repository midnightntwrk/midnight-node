#node
# Add transaction validation caching

Introduces a two-tier caching mechanism for transaction validation to eliminate
redundant cryptographic verification work. Build to ensure invalid transactions
never make it to the block, whilst improving performance of the mempool.

Includes Prometheus metrics (`midnight_ledger_tx_*_cache_hits/misses`) and debug
logging for monitoring cache effectiveness.

PR: https://github.com/midnightntwrk/midnight-node/pull/608
Ticket: https://shielded.atlassian.net/browse/PM-21592
