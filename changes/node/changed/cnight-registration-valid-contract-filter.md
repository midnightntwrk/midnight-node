#cnight-observation #federated-authority #db-sync
# Exclude phase-2-invalid tx outputs from script-address observation queries

db-sync stores a phase-2-failed transaction's collateral-return output as a
regular `tx_out` row, so a UTxO created that way at an observed script address
was previously indistinguishable from a normal output. Observation queries now
require `tx.valid_contract = true` on the producing tx:

- `get_registrations`, with the symmetric filter in `get_deregistrations` on
  the spent output's origin tx (plus the spending tx, which cannot be invalid
  for script-locked outputs) so a UTxO ignored at creation is also ignored
  when spent.
- `get_governance_body_utxo` (council / technical committee observation) and
  the genesis-verification `get_authorization_script_from_datum`, so the
  governance datum is never read from an output created by an invalid tx.
- `get_token_utxo_for_epoch` (D-parameter / permissioned-candidates datum, both
  the midnight candidates data source and the vendored partner-chains copy).

cNIGHT asset create/spend tracking, UTxO-set queries and bridge transfer
observation (`get_bridge_txs`) are intentionally left unfiltered: db-sync stores
an invalid tx's actual ledger effects (collateral consumed as `tx_in`, collateral
return as `tx_out`), so filtering there would desync balance accounting from the
real Cardano UTxO set. In particular, tokens a collateral return moves to the
bridge address are genuinely locked on Cardano and must surface on Midnight
(user transfer, reserve, or treasury via `TransferRecipient::Invalid`) to
preserve the bridge accounting invariant.

PR: <link to PR>
