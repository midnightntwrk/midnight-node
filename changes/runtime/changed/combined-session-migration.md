#runtime #migration #committee-selection

# Combine committee v1-to-v2 and session-key migrations in the runtime

This chain moves `pallet-session-validator-management` from v1 to v2 in a single
upgrade, which spans two changes the toolkit ships as separate migrations: the
`AuthorityKeys` shape change (`migrations::authority_keys`) and v2's new
`QueuedCommittee` (`migrations::v2`). They cannot be wired one after the other,
so a custom migration that combines both is added.

PR: https://github.com/midnightntwrk/midnight-node/pull/1953
Issue: https://github.com/midnightntwrk/midnight-node/issues/1742
