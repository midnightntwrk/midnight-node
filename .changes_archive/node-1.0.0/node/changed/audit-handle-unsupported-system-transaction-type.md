#audit #client
# Reject unsupported system transaction types

The `get_system_tx_type` function previously used a wildcard match arm that
silently labeled unrecognized `SystemTransaction` variants as `"unknown"` for
Prometheus metrics. This changes the function to return an explicit error
(`SystemTransactionError::UnknownError`, code 204) for unrecognized variants,
ensuring they are rejected before processing. Known variants are unaffected.

PR: https://github.com/midnightntwrk/midnight-node/pull/840
Ticket: https://shielded.atlassian.net/browse/PM-19971
