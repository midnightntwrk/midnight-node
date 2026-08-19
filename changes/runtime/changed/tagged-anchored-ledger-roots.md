#runtime #ledger
# Anchored ledger tips are tagged with the block hash after import

Post-block and genesis ledger states are first persisted as a raw GC root
(`on_finalize` does not yet know the block hash). After every executed import
(including initial sync), the native node stages a swap to a content-addressed
wrapper (`block hash` + inner state); the next `on_finalize` flush commits it.
Sibling forks get distinct wrappers and can be released independently.
Intra-block Transient states are unchanged.

Requires a resync from genesis; pre-wrapper anchored roots are not migrated.

PR: <link to PR>
