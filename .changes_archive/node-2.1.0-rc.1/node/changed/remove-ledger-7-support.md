#node #toolkit #runtime
# Remove ledger 7 support

Mainnet has always run ledger 8 onwards, so ledger 7 (a pre-mainnet protocol
generation) is dead code. Removed the `ledger_7` host bridge, storage,
validation, error, and system-tx modules from the `ledger` and
`ledger/helpers` crates, and all ledger-7 builders/commands from the toolkit.
`ForkAwareLedgerContext::dispatch` now takes two closures (ledger 8, ledger 9)
instead of three, and `LedgerVersion` no longer has a `Ledger7` variant.

This is a full purge, not a version-support drop: the node can no longer
decode or replay chain history that predates the ledger-7→8 hardfork
(mainnet is unaffected since it launched post-hardfork; devnet/testnet
archives from before the hardfork are no longer replayable by the toolkit).

`LedgerVersion` now carries explicit wire discriminants (`Ledger8 = 1`,
`Ledger9 = 2`) with hand-written serde impls, so dropping the `Ledger7`
variant does not renumber the postcard encoding used by the
`raw_block_data_v2` block cache. Existing redb and PostgreSQL caches keep
decoding as the ledger they were written with; a cached block that really is
ledger 7 (discriminant 0) is now rejected with an explicit "delete the cache
and re-fetch" error rather than silently replaying as ledger 8.

PR: https://github.com/midnightntwrk/midnight-node/pull/1999
