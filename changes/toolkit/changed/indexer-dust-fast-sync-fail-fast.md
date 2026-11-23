#toolkit #indexer
# Retry DUST fast sync instead of replaying it on indexer transport errors

When DUST fast sync on the `--indexer-url` path hit an HTTP or WebSocket error, such as a 502
while an indexer API pod restarted, the toolkit fell back to replaying the chain-wide DUST event
log. On a chain forked from mainnet that is about 2.3 million events per wallet, takes minutes,
and adds heavy load to an indexer that is already struggling. The toolkit now retries the fast
sync up to four times, waiting 10, 20 and 30 seconds in between, and then fails with the transport
error rather than replaying. Responses the toolkit cannot use, and indexers without fast-sync
support, still fall back to the replay.
