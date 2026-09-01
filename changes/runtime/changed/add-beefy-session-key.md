#runtime #session-keys #beefy
# Add the BEEFY session key

`opaque::SessionKeys` gains `beefy`, translated by the same
`AddBabeToSessionKeysMigration` cutover as the BABE key. The placeholder
is the validator's aura bytes behind the invalid SEC1 tag `0x00`
(distinct per validator, collides with no real key); candidate
registrations without a BEEFY key fall back to the same derivation.
Genesis takes the configured `beefy_pubkey`.

PR: https://github.com/midnightntwrk/midnight-node/pull/1953
Issue: https://github.com/midnightntwrk/midnight-node/issues/1742
