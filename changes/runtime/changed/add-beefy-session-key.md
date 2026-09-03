#runtime #session-keys #beefy

# Add the BEEFY session key

`opaque::SessionKeys` gains `beefy`, translated by the
`AddBeefyToSessionKeysMigration` cutover, which gives each validator its own
cross-chain key as its beefy key. Both are ECDSA and the committee registers
them as equal (`beefy_pub_key == sidechain_pub_key`), so the migrated
authority set is one the validators actually hold secrets for — a derived
placeholder would leave BEEFY unable to reach a quorum until the next
committee rotation.

Because BEEFY is now a session key, `pallet_session`'s genesis initializes
the BEEFY authorities from the committee, so the chain spec no longer sets
`BeefyConfig::authorities` itself — see the separate genesis change.

The runtime version moves to `002_001_001`.

PR: https://github.com/midnightntwrk/midnight-node/pull/2084
Issue: https://github.com/midnightntwrk/midnight-node/issues/1742
