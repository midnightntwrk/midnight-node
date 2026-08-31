#runtime #migration #session-keys #babe #beefy
# Add babe and beefy session keys and wire the combined v1-to-v2 migration

`opaque::SessionKeys` gains `babe` and `beefy`. The combined
`V1ToV2Migration` plus `SetAddBabeSessionKeysMigratedFlag` are wired into
`SingleBlockMigrations`: one versioned step translates committee and
`pallet_session` key storage from the aura+grandpa shape, seeds
`QueuedCommittee`, and records the consensus-engine guard the node-side
committee decoders read. Placeholder keys are distinct per validator:
babe reuses the aura key, beefy prefixes the aura bytes with the invalid
SEC1 tag `0x00`. Candidate registrations without BABE/BEEFY keys fall
back to the same derivations.

PR:
Issue: https://github.com/midnightntwrk/midnight-node/issues/1742
