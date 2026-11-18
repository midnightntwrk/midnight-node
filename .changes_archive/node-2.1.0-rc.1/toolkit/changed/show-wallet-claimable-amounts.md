#toolkit

# `show-wallet` reports claimable block-reward and bridge-transfer amounts

`toolkit show-wallet` now includes two additional fields in its JSON output:

- `claimable_block_rewards` — NIGHT block-production rewards currently claimable by
  the wallet's unshielded address (from the ledger `unclaimed_block_rewards` map).
- `claimable_bridge_transfers` — NIGHT bridged from Cardano via the protocol
  bridge and currently claimable by the wallet's unshielded address, already net of
  the bridge fee (from the ledger `bridge_receiving` map).

PR: https://github.com/midnightntwrk/midnight-node/pull/1766
Issue: https://github.com/midnightntwrk/midnight-node/issues/1765
