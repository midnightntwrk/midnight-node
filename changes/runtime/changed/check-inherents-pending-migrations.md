#runtime #consensus

# Apply pending migrations inside `check_inherents`

`BlockBuilder::check_inherents` now runs `Executive::execute_on_runtime_upgrade()` first
when `LastRuntimeUpgrade` shows the runtime was just upgraded, before checking the
block's inherents. The writes are discarded with the runtime API call; the block's own
execution runs the migrations again, for real.

This is the runtime half of the fix for the first post-upgrade block being rejected by
verifiers (see the node change file "Check block inherents in the on-chain call
context"). With the node checking that block in the on-chain context, the new runtime
performs the check against the parent state the old runtime left behind, so the inherent
checks would otherwise read unmigrated storage (`Corrupted state`, `InvalidValidators`).

PR:
Issue:
