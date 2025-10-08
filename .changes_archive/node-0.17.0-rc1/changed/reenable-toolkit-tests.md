# Reenable tests inside of `midnight-node-toolkit`

Re-enables several toolkit tests which were disabled when the ledger was updated to v6. Also updates the tests so that whenever genesis is rebuilt, the test data is rebuilt as well, so they will be easy to maintain in the future.

PR: https://github.com/midnightntwrk/midnight-node/pull/29
JIRA: https://shielded.atlassian.net/browse/PM-18995
