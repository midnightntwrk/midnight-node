#node #ledger
# Anchored ledger tips are tagged with the block hash after import

The node watches block imports and swaps each Anchored ledger persist for a
wrapper tagged with that block's header hash. Reclaim can later `release_tagged`
by hash. Transient intra-block states still use a raw persist.

Requires a resync from genesis; pre-wrapper anchored roots are not migrated.

PR: <link to PR>
