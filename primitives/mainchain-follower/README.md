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

Observation queries that interpret outputs at watched script addresses (registrations,
deregistrations, governance body UTxOs, committee-selection token data, bridge
transfers) filter on `tx.valid_contract = true` to safeguard against malicious use of
failing script transactions: an attacker could otherwise use a failing transaction's collateral
return to place a crafted output at an observed address. Tokens moved to the bridge
address this way are deliberately never observed and stay locked.

Queries that track raw balances or compute UTxO sets (asset creates/spends, UTxOs at
an address) intentionally do **not** filter: db-sync rows for invalid transactions
describe real on-chain effects, and excluding them would desync the observed state
from the actual Cardano UTxO set.
