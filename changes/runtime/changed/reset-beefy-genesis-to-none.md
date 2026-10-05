#runtime #beefy #migration

# Reset the BEEFY genesis block to `None` on runtime upgrade

Adds a one-shot runtime migration that clears `pallet_beefy::GenesisBlock`, so
`BeefyApi::beefy_genesis` returns `None` and the BEEFY gadget on every node stays idle instead
of waiting forever on the mandatory block 1. That block can only be signed by the chain-spec
genesis authority set, which no validator holds keys for. Adding `beefy` to `SessionKeys` in
the same upgrade makes `pallet_beefy::Authorities` follow the committee from the next session
rotation on, but does not change who owns the session that starts at block 1.

BEEFY remains disabled until governance re-enables it with `pallet_beefy::set_new_genesis`,
which starts a fresh first session at a future block with the validator set active there. This
should happen only after at least one session rotation has followed the upgrade. The migration
is a no-op once the value is already `None` and must be removed before BEEFY is re-enabled.

PR: https://github.com/midnightntwrk/midnight-node/pull/2084
Issue: https://github.com/midnightntwrk/midnight-node/issues/1742
