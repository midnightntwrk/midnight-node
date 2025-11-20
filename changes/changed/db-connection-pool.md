#audit
# Connection Pools for DataSources

Previously, we were using a single (rather small) connection pool for all of our data sources. This means that if we have lots of RPC requests, consensus would have to wait for a free connection.

Now, each data source has it's own connection pool, where each connection pool can be fine-tuned for ideal performance.

Ticket: https://shielded.atlassian.net/browse/PM-19903
PR: https://github.com/midnightntwrk/midnight-node/pull/267
