#runtime
# Per-pallet allow-listed system transaction executors

`MidnightSystemTransactionExecutor` let any caller submit any `SystemTransaction`
variant to the ledger, with no restriction beyond the root-origin extrinsic's own
governance allow-list. `pallet-cnight-observation` and `pallet-c2m-bridge` now go
through separate `MidnightSystemTransactionCNightExecutor` and
`MidnightSystemTransactionBridgeExecutor` traits, each backed by a dedicated ledger
host function (`apply_cnight_system_transaction`, `apply_bridge_system_transaction`)
that only accepts the system transaction variants that pallet is allowed to
construct. The governance extrinsic path was rebuilt the same way, onto a new
`apply_governance_system_transaction` host function. The previous
`apply_system_transaction` and `is_governance_allowed_system_tx` host functions are
kept, unused by new code, for backward compatibility with already-published runtime
WASM and historical block replay.

PR: https://github.com/midnightntwrk/midnight-node/pull/2080
