#runtime #ledger
# Anchored ledger tips are tagged with the block hash after import

Post-block and genesis ledger states are first persisted as a raw GC root
(`on_finalize` does not yet know the block hash). After every executed import
(including initial sync), the native node stages a swap to a content-addressed
wrapper (`block hash` + inner state); the next `on_finalize` flush commits it.
Warp ledger-sync persists the recovered arena already tagged with the warp
target hash, in the snapshot import flush. Sibling forks get distinct wrappers
and can be released independently. Intra-block Transient states are unchanged.
Pre-v3 block tips are tagged on a historical full sync; pre-v3 intra-block
intermediates still leak (v1 host functions persist every successor).

Requires a resync from genesis; pre-wrapper anchored roots are not migrated.

PR: <link to PR>
