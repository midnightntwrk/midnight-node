#runtime #ledger
# Anchored ledger tips are tagged with the block number at persist time

Post-block and genesis ledger states are persisted as a content-addressed
wrapper tagged with `System::Number`. Warp ledger-sync persists the
recovered arena already tagged with the warp target number. Intra-block
Transient states are unchanged. Forks at the same height share a tag.

Requires a resync from genesis; pre-wrapper anchored roots are not migrated.

PR: <link to PR>
