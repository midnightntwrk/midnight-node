#toolkit #ledger #ecdsa #contract-maintenance

# ECDSA contract maintenance & deploy committees

Extends the toolkit's ECDSA unshielded-signature support to contract maintenance and deploy
committees, completing the ledger-9 ECDSA story (previously committees were Schnorr-only).

- Committee members are now full `UnshieldedWallet`s rather than bare Schnorr keys, so each
  member's signature scheme (Schnorr or ledger-9 ECDSA) travels with it. The ledger-helpers
  transaction-building info types (`MaintenanceUpdateInfo`, `ContractMaintenanceAuthorityInfo`,
  `ContractDeployInfo`) and the `Contract::deploy` trait now take committee wallets and derive the
  maintenance verifying key / sign via the wallet's scheme-agnostic accessors.
- New `UnshieldedWallet::maintenance_verifying_key()` dispatches to the Schnorr or ECDSA
  `ContractMaintenanceVerifyingKey` variant, backed by a per-generation `MaintenanceVerifyingKey`
  type alias (the plain signature verifying key pre-ledger-9, the maintenance-key enum on 9).
- CLI: `--authority-seed` / `--new-authority-seed` (on `generate-txs contract deploy` and
  `contract maintenance`) now accept the same optional `schnorr:`/`ecdsa:` scheme prefix as the
  other seed flags (bare = Schnorr, backwards compatible). Mixed-scheme committees are supported.
- ECDSA committees are rejected on pre-ledger-9 chains via the shared `relevant_wallet_schemes`
  guard, giving a clear CLI error instead of a panic deep in the ledger-7/8 ECDSA stubs. The
  contract *funding* seed stays Schnorr.

Tests: unit coverage for scheme-dispatched address/maintenance-key derivation, ECDSA sign/verify
round-trip, and tagged wallet (de)serialization (`ledger/helpers`); plus a ledger-acceptance test
that the ledger's `SignatureKind::signature_verify` (the primitive `Transaction::well_formed` runs)
accepts the toolkit's wrapped ECDSA signature and rejects cross-scheme mismatches (`util/toolkit`).

PR: https://github.com/midnightntwrk/midnight-node/pull/1861
Issue: https://github.com/midnightntwrk/midnight-node/issues/1542
