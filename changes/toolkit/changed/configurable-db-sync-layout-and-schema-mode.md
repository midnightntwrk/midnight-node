#toolkit
# Support configurable db-sync layouts and operator-managed indexes

Partner Chains db-sync data sources now support both transaction-input representations (`tx_in`
and `tx_out.consumed_by_tx_id`) and both address representations (inline `tx_out.address` and the
normalized `address` table). Public configuration types allow callers to select an explicit
layout or retain automatic transaction-input detection.

Candidate data sources also support `apply`, read-only `verify`, and `skip` index policies. The
runtime manifest includes the selected address and transaction-input indexes, accepts equivalent
operator-managed indexes regardless of name, and preserves the existing automatic behavior by
default for initialized databases. Ambiguous empty input layouts now require an explicit mode.

Issue: https://github.com/midnightntwrk/midnight-node/issues/1160
