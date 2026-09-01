#runtime #session-keys #beefy
# Add the BEEFY session key

`opaque::SessionKeys` gains `beefy`, translated by the
`AddBeefyToSessionKeysMigration` cutover. The placeholder for validators
already in the committee is the validator's aura bytes behind the invalid
SEC1 tag `0x00` (distinct per validator, collides with no real key);
candidate registrations without a BEEFY key fall back to the same
derivation.

Because BEEFY is now a session key, `pallet_session`'s genesis initializes
the BEEFY authorities from the committee, so the chain spec no longer sets
`BeefyConfig::authorities` itself — see the separate genesis change.

The runtime version moves to `002_001_001`.

PR: https://github.com/midnightntwrk/midnight-node/pull/1953
Issue: https://github.com/midnightntwrk/midnight-node/issues/1742
