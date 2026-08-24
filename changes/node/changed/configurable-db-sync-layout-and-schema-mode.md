#node
# Support configurable db-sync layouts and read-only schema verification

Add explicit node configuration for transaction-input storage (`auto`, `tx_in`, or `consumed`),
address storage (`inline` or `address_table`), and schema management (`apply`, `verify`, or
`skip`). Midnight data sources now adapt their queries to supported cardano-db-sync layouts and
can verify operator-managed indexes without requiring database write privileges. The existing
`auto`/`inline`/`apply` behavior remains the default for initialized databases; ambiguous empty
input layouts now fail with an actionable request for explicit configuration.

Issue: https://github.com/midnightntwrk/midnight-node/issues/1160
