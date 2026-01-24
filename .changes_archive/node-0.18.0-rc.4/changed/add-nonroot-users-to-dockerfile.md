# Add nonroot users to all Dockerfiles

The `midnight-node`, `midnight-node-toolkit`, and `hardfork-test-upgrader` images all run as a user named `appuser` by default.

PR: https://github.com/midnightntwrk/midnight-node/pull/114
Ticket: https://shielded.atlassian.net/browse/SEC-1062
Ticket: https://shielded.atlassian.net/browse/SEC-1063
Ticket: https://shielded.atlassian.net/browse/SEC-1064