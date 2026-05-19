#audit
# Add test coverage for UtxoOwners persistence guards

Tests that event construction failure does not leave orphaned UtxoOwners entries,
and that spending a UTXO without a prior create does not emit a Destroy event.

PR: https://github.com/midnightntwrk/midnight-node/pull/762
JIRA: https://shielded.atlassian.net/browse/PM-20218
