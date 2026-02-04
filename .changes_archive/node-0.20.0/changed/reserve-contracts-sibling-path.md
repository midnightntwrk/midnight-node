#local-environment
# Use sibling directory for midnight-reserve-contracts path

Change the MIDNIGHT_RESERVE_CONTRACTS_PATH environment variable to reference the
contracts repository as a sibling directory rather than a child directory. This
aligns with typical developer workspace layouts and matches the CI configuration.

PR: https://github.com/midnightntwrk/midnight-node/pull/510
JIRA: https://shielded.atlassian.net/browse/PM-21404
