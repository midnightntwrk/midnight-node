#node

# Support configurable db-sync layouts and read-only schema verification

Add explicit node configuration for transaction-input storage (`auto`, `tx_in`, or `consumed`),
address storage (`inline` or `address_table`), and schema management (`apply`, `verify`, or
`skip`). Midnight data sources now adapt their queries to supported cardano-db-sync layouts and
can verify operator-managed indexes without requiring database write privileges. The existing
`auto`/`inline`/`apply` behavior remains the default for initialized databases; ambiguous empty
input layouts now fail with an actionable request for explicit configuration.

Share index specifications between runtime and genesis manifests, retaining the
`tx_out(data_hash)` index for cNight genesis without adding it to normal node startup.

# Support configurable db-sync layouts and operator-managed indexes

Partner Chains db-sync data sources now support both transaction-input representations (`tx_in`
and `tx_out.consumed_by_tx_id`) and both address representations (inline `tx_out.address` and the
normalized `address` table). Public configuration types allow callers to select an explicit
layout or retain automatic transaction-input detection.

Candidate data sources also support `apply`, read-only `verify`, and `skip` index policies. The
runtime manifest includes the selected address and transaction-input indexes, accepts equivalent
operator-managed indexes regardless of name, and preserves the existing automatic behavior by
default for initialized databases. Ambiguous empty input layouts now require an explicit mode.

PR: https://github.com/midnightntwrk/midnight-node/pull/2199
Issue: https://github.com/midnightntwrk/midnight-node/issues/1160
