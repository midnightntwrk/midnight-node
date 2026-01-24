#toolkit
# Correctly handle partial successes in `midnight-node-toolkit`

If only some intents from a transaction were applied, the toolkit should only keep track of some fallible coins.

PR: https://github.com/midnightntwrk/midnight-node/pull/104
JIRA: https://shielded.atlassian.net/browse/PM-19526