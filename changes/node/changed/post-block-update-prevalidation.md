#node #runtime
# Prevalidate post-block-update after each ledger 9 transaction

Transaction application (`apply_verified_transaction`, `apply_system_tx`) now
runs ledger 9's `prevalidate_post_block_update` before mutating state, so
transactions that would fail end-of-block processing are rejected early.

Ledger 7 and 8 keep the previous behaviour via version-specific
`post_block_update` modules (no-op; validation still runs only at
`post_block_update`).

PR: https://github.com/midnightntwrk/midnight-node/pull/1448
