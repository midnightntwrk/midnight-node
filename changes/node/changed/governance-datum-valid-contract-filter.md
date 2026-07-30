#federated-authority #db-sync
# Exclude phase-2-invalid tx outputs from governance-datum observation queries

db-sync stores a phase-2-failed transaction's collateral-return output as a
regular `tx_out` row, so a UTxO created that way at an observed script address
was previously indistinguishable from a normal output. Governance-datum queries
now require `tx.valid_contract = true` on the producing tx as a conservative
safeguard, since these datums are trusted by the presence of a governance token
rather than by their contents:

- `get_governance_body_utxo` (council / technical committee observation) and
  the genesis-verification `get_authorization_script_from_datum`.
- `get_token_utxo_for_epoch` (D-parameter / permissioned-candidates datum, both
  the midnight candidates data source and the vendored partner-chains copy).

Registration/deregistration observation and all value-accounting queries
(cNIGHT asset create/spend tracking, UTxO-set queries, bridge transfers) are
intentionally left unfiltered: db-sync rows for invalid transactions describe
real ledger effects (collateral consumed as `tx_in`, collateral return as
`tx_out`), so filtering would desync observation from the real Cardano UTxO
set. See "Handling of failed script transactions" in the mainchain-follower
README for the full rationale.

PR: https://github.com/midnightntwrk/midnight-node/pull/1939
