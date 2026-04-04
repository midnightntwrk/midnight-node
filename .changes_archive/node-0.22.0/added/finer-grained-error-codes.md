#node
# Finer grained ledger error codes

Map all known MalformedTransaction and TransactionInvalid variants to specific error codes instead of falling through to UnknownError. Fixes the u8 collision between MalformedError::UnknownError and SystemTransactionError::IllegalPayout (both previously mapped to 139). Adds a test to prevent future collisions.

PR: https://github.com/midnightntwrk/midnight-node/pull/745
JIRA: https://shielded.atlassian.net/browse/PM-21798
