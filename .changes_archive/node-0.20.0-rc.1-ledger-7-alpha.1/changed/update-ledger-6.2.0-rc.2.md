# Update Ledger to 6.2.0-rc.2

Updates midnight-node to be compatible with midnight-ledger version 6.2.0-rc.2, which introduced several breaking API changes:

- `LedgerState::post_block_update` now requires 3 arguments: `Timestamp`, `NormalizedCost`, `FixedPoint`
- `StorageBackend::pre_fetch` now takes `&ArenaHash` instead of `&ArenaKey`
- `Sp::persist` now requires `&mut self`
- `FeePrices` structure redesigned: field names changed from `*_price` to `*_factor` with new `overall_price` field

Jira: https://shielded.atlassian.net/browse/PM-20907

PR: https://github.com/midnightntwrk/midnight-node/pull/352
