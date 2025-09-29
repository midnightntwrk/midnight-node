# Fix toolkit - use cngd event

The toolkit created invalid proofs as a result of its state being different from that of the node's. The toolkit needs to observe system tx events of cngd in order to stay in line.

PR: https://github.com/midnightntwrk/midnight-node/pull/60
JIRA: https://shielded.atlassian.net/browse/PM-19370