#runtime #session-keys #beefy

# Add the BEEFY session key

`opaque::SessionKeys` gains `beefy`.
`MigrateV1ToV2AddBabeAndBeefySessionKeys` gives each validator its own
cross-chain key as its beefy key. Both are ECDSA and the committee registers
them as equal (`beefy_pub_key == sidechain_pub_key`).

Because BEEFY is now a session key, `pallet_session`'s genesis initializes
the BEEFY authorities from the committee, so the chain spec no longer sets
`BeefyConfig::authorities` itself. This is a change in genesis creation
that matches what is done for AURA, BABE and GRANDPA.

PR: https://github.com/midnightntwrk/midnight-node/pull/2084
Issue: https://github.com/midnightntwrk/midnight-node/issues/1742
