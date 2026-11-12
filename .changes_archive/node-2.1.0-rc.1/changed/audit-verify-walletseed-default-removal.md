#ledger
# Verify removal of WalletSeed Default implementation

Verify the audit finding (A2 Issue D) remediation from PR #804 that removed
the all-zero Default implementation for WalletSeed. Confirms no residual
zero-seed usage in key derivation paths.

PR: https://github.com/midnightntwrk/midnight-node/pull/1109
Issue: https://github.com/midnight-security/midnight-security/issues/112
JIRA: https://shielded.atlassian.net/browse/PM-22024
