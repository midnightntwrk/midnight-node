#toolkit
# Add indexer-backed `show-wallet` (`--indexer-url`)

`show-wallet` now accepts `--indexer-url <URL>` (env `MN_INDEXER_URL`) to reconstruct wallet
state from a Midnight indexer's GraphQL API (`api/v4`) instead of replaying every block from the
node, which is slow and grows with chain length (issue #1186). Use it with `--seed` and
`--network`; it produces the same `WalletInfoJson` (shielded coins, unshielded UTXOs, dust UTXOs)
as the replay path.

This is the foundation of the indexer-backed toolkit: it adds the GraphQL client (HTTP queries
plus `graphql-transport-ws` subscriptions) and the wallet sync (`connect`, draining the
shielded/unshielded/dust subscriptions to the chain tip) in a new `IndexerContext`. Building
transactions over the indexer is a follow-up that reuses everything here. Only the latest ledger
version (v9) is supported on the indexer path; the existing multi-version replay path is unchanged.

The client's GraphQL operations are typed against the indexer's committed schema
(`indexer/indexer-api/graphql/schema-v4.graphql`, read from the submodule at compile time via
`graphql_client`), so an incompatible schema change in the submodule breaks the build rather than
failing at runtime. The whole indexer client lives behind a new `indexer-client` cargo feature —
on by default for the toolkit, off for `midnight-node-ledger-helpers` — so the node and other
consumers still build without the indexer submodule. `--workspace`/toolkit builds (which enable
the feature) need the submodule checked out; the relevant Earthly targets copy just the schema
file, and `tests/e2e` opts out of the feature.

An end-to-end test (`util/toolkit/tests/indexer_show_wallet_e2e.rs`) spins up a node plus
`indexer-standalone` on a shared Docker network and asserts the synced wallet reports the funded
genesis balances. It is gated behind `MN_RUN_INDEXER_E2E=1` until the `indexer-standalone` image is
published and pinned in CI (set `--INDEXER_STANDALONE_IMAGE` on the `+test-toolkit` Earthly target,
which then enables the test).

PR: <link to PR>
Issue: https://github.com/midnightntwrk/midnight-node/issues/1186
