# Add nonroot users to all Dockerfiles

The `midnight-node`, `midnight-node-toolkit`, and `hardfork-test-upgrader` images all run as a user named `appuser` by default.

Note that on Linux, nonroot users don't have access to mounted Docker volumes. If you are mounting volumes into a container, you must run that container as root.

PR: https://github.com/midnightntwrk/midnight-node/pull/43
Ticket: https://shielded.atlassian.net/browse/SEC-1062
Ticket: https://shielded.atlassian.net/browse/SEC-1063
Ticket: https://shielded.atlassian.net/browse/SEC-1064