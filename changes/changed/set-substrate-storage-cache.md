#client
# Set Substrate storage Trie cache back to default values

When we were storing the entire ledger state as a single substrate storage item,
we had to set the trie_cache_size to zero to workaround memory usage issues
(Substrate was rightly never designed to store lots of data as one storage item).

We've since fixed this restriction, so we can set the trie_cache_size back to a sensible value.

PR: https://github.com/midnightntwrk/midnight-node/pull/601
Ticket: https://shielded.atlassian.net/browse/PM-16976
