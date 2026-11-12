#node
# Early weight check in midnight pallet pre_dispatch

Add an early block weight check in `ValidateUnsigned::pre_dispatch` before
expensive ledger validation. Substrate's `Bare` extrinsic path runs the pallet's
`pre_dispatch` before the `CheckWeight` extension, which means transactions that
won't fit in the block still undergo costly ledger validation before being
rejected. The new check mirrors the logic in `calculate_consumed_weight` and
exits early with `ExhaustsResources` when the block is full.

PR: https://github.com/midnightntwrk/midnight-node/pull/1305