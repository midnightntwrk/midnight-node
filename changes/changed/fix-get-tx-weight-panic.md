#node
# Fix WASM trap in transaction validation when cost exceeds block limits

Fixed a panic in `get_tx_weight` that caused WASM traps during transaction validation. When `get_transaction_cost` failed (e.g., due to `BlockLimitExceededError`), the `.expect()` call would panic in `get_dispatch_info` before the transaction reached `validate_unsigned` where the error would be properly handled.

The fix replaces `.expect()` with `EXTRA_WEIGHT_TX_SIZE`, allowing the transaction to proceed to validation where `BlockLimitExceededError` is properly returned as an `InvalidTransaction` error.

PR: https://github.com/midnightntwrk/midnight-node/pull/489
JIRA: https://shielded.atlassian.net/browse/PM-21219
