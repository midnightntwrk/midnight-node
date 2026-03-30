#node #runtime
# Fix runtime-call panics caused by uninitialized storage during runtime upgrade

Before this fix, ledger storage was only re-initialized at the start of each
block. This allowed for a small time window after a runtime upgrade where the
storage for the new ledger version is uninitialized.

This change closes this window by always checking storage initialization on a
runtime upgrade.

PR: https://github.com/midnightntwrk/midnight-node/pull/870
Fixes: https://shielded.atlassian.net/browse/PM-22228
