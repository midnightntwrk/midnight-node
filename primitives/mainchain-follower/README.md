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
with `tx.valid_contract = false`. The transaction's declared outputs (which never
materialize on-chain) are discarded and never appear in `tx_out`, and its declared
inputs are never recorded as spent. Every `tx_out`/`tx_in` row therefore describes a
real on-chain effect regardless of `valid_contract`.

Most observation queries consequently do **not** filter on `tx.valid_contract`, since
excluding those rows would desync the observed state from the actual Cardano UTxO set:

- Registrations/deregistrations: a collateral return can carry a datum and tokens (it
  is a full Babbage-era output), so a UTxO created that way at the registration
  address is a genuine — if convoluted — registration; the datum is
  self-authenticating (it carries user signatures) and creating a UTxO at the address
  is permissionless anyway. On the spending side, script-locked UTxOs can never be
  consumed by an invalid transaction: the ledger forbids script-locked collateral
  inputs, and an invalid transaction consumes only its collateral.
- Asset creates/spends, UTxO-set queries, and bridge transfers (`get_bridge_txs`):
  tokens moved by a collateral return to the bridge address are genuinely locked on
  Cardano, and the bridge invariant requires every locked token to surface on
  Midnight — as a user transfer, a reserve distribution, or a treasury unlock
  (`TransferRecipient::Invalid` catches deposits without valid metadata). Filtering
  would silently strand locked tokens.

Governance-datum queries (`get_governance_body_utxo`, the genesis-verification
`get_authorization_script_from_datum`, and `get_token_utxo_for_epoch` for the
D-parameter / permissioned-candidates data) **do** filter on
`tx.valid_contract = true` as a conservative safeguard: these datums are trusted by
the presence of a governance token rather than by their contents, so the datum is
only read from outputs of transactions that fully validated.
