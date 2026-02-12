#node #ledger #client
# Call gc() after flushing ledger storage to reclaim arena memory

During block sync, post_block_update materializes trie nodes that accumulate in the arena
and are never freed, causing linear heap growth. Adding backend.gc() after
flush_all_changes_to_db() allows the arena to reclaim unreachable nodes each block,
reducing the memory leak rate by ~50% and changing heap behavior from unbounded linear
growth to active reclamation.

PR: https://github.com/midnightntwrk/midnight-node/pull/657
Ticket: https://shielded.atlassian.net/browse/PM-21764
