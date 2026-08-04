#node
# Remove hard-fork test ledger version dependencies

Remove the `*-hf` ledger dependencies and all `cfg(hardfork_test)` /
`cfg(hardfork_test_rollback)` conditional compilation infrastructure.
Hard-fork e2e tests no longer require building a separate node with an
older ledger version.

PR: https://github.com/midnightntwrk/midnight-node/pull/1024
JIRA: https://shielded.atlassian.net/browse/PM-22109
