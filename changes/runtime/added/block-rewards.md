#runtime #rewards
# Add native NIGHT block rewards

New `pallet_block_rewards` draws each block's reward and settles every Cardano
epoch to the Treasury and to stake pool payees through the ledger's reward system
transactions. It stays inactive until governance calls `activate`, which needs
the stake snapshot and the reserve release.

PR: https://github.com/midnightntwrk/midnight-node/pull/2473
