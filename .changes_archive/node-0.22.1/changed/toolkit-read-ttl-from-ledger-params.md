#toolkit
# When building transations use global_ttl from the ledger parameters instead of hardcoded 10 minutes

Changes the toolkit to use ledger_parameters.global_ttl to compute transaction ttl instead of hardcoded 10 minutes.

PR: https://github.com/midnightntwrk/midnight-node/pull/791
Ticket: https://shielded.atlassian.net/browse/PM-21976
