# Compile-time checked database queries

The database queries in this repo are checked at compile-time. When changing a query, the metadata for that query must be re-generated. This can be done via earthly:

```bash
$ earthly +rebuild-sqlx
```

**NOTE:** `local-env` must be running for this to work! `earthly +start-local-env-latest`

# Handling of failed script transactions (`tx.valid_contract`)

For a transaction that fails phase-2 script validation, cardano-db-sync stores the
actual ledger effects in the regular tables: the consumed collateral inputs become its
`tx_in` rows and the collateral-return output becomes a regular `tx_out` row, flagged
with `tx.valid_contract = false`.

Observation queries that interpret *datums* at watched script addresses (registrations,
deregistrations, governance body UTxOs, committee-selection token data) filter on
`tx.valid_contract = true` to safeguard against malicious use of failing script
transactions: an attacker could otherwise use a failing transaction's collateral
return to place a crafted datum at an observed address.

Queries that account for *value* intentionally do **not** filter: db-sync rows for
invalid transactions describe real on-chain effects, and excluding them would desync
the observed state from the actual Cardano UTxO set. This covers asset creates/spends,
UTxO-set queries, and bridge transfers (`get_bridge_txs`): a collateral return that
moves tokens to the bridge address is a genuine lock on Cardano, and the bridge
invariant requires every locked token to surface on Midnight — as a user transfer,
a reserve distribution, or a treasury unlock (`TransferRecipient::Invalid` catches
deposits without valid metadata). Filtering there would silently strand locked tokens.
