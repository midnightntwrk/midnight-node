#node #runtime
# Gate the tblock correction on a runtime upgrade instead of a date

The tblock correction — verifying a block's *first* ledger transaction at
`parent_block_time + 12s` so historical blocks whose transactions were served from the producing
node's warm strict cache still import — used to switch itself off at a hardcoded date,
`tblock_correction_disable_after`. That date had to be pushed back indefinitely or syncing from
genesis would break, and both it and `tblock_correction_offset` were consensus-critical values
read from node config, where a single validator with a different `default.toml` would verify
historical blocks differently from its peers.

The correction is now gated on the on-chain runtime. Version 1 of the `Ledger8Bridge`
`apply_transaction` and `validate_guaranteed_execution` host functions applies it; version 2,
added here, does not. Historical blocks replay against whichever runtime was on-chain at that
height, so pre-upgrade wasm imports version 1 and still corrects, while every block from the
`set_code` onward is verified against its own timestamp. The offset is hardcoded at 12 seconds
(`slot_duration_secs * (1 + MaxSkippedSlots)`, fixed for every chain the correction can apply to),
and `tblock_correction_offset` and `tblock_correction_disable_after` are removed from node config
along with the externalities extension that carried them.

The `Ledger9Bridge` host functions never skew: every ledger-9 block is produced after the upgrade
that closed the loophole.

**All validators must be running a node that exports
`ext_ledger_8_bridge_apply_transaction_version_2` (2.1.0, or 1.0.3 on the release branch) before
the upgrade is enacted.** An older node cannot instantiate a runtime that imports it, so any
validator left behind stops importing blocks at the `set_code`.

Backport of https://github.com/midnightntwrk/midnight-node/pull/2002.

Issue: https://github.com/midnightntwrk/midnight-node/issues/1924
