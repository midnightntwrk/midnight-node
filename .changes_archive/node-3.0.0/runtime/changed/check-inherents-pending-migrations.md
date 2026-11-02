#runtime #consensus

# Apply pending migrations inside `check_inherents`

`BlockBuilder::check_inherents` now runs `Executive::execute_on_runtime_upgrade()` first
when `LastRuntimeUpgrade` shows the runtime was just upgraded, before checking the
block's inherents. The runtime storage writes are discarded with the runtime API call; the
block's own execution runs the migrations again, for real.

Host-side effects are not discarded. A migration that acts through a host function (e.g. the
ledger v8→v9 translation, which persists the translated state in the ledger arena) now runs
twice on every verifying node for the first post-upgrade block: once in this check and once
in the block's execution. Migrations must therefore be deterministic and idempotent on the
host side. This was already required, since competing children of the upgrade block,
abandoned proposals and resyncs re-run them on the same host state, but it is now the normal
path rather than an edge case.

This is the runtime half of the fix for the first post-upgrade block being rejected by
verifiers (see the node change file "Check block inherents in the on-chain call
context"). With the node checking that block in the on-chain context, the new runtime
performs the check against the parent state the old runtime left behind, so the inherent
checks would otherwise read unmigrated storage (`Corrupted state`, `InvalidValidators`).

PR: https://github.com/midnightntwrk/midnight-node/pull/2252
Issue: n/a
