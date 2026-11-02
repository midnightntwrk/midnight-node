#toolkit #ledger #ecdsa #e2e #contract-maintenance

# ECDSA toolkit end-to-end coverage

Adds an end-to-end test (`ecdsa_contract_committees_e2e` in `util/toolkit/tests/toolkit_e2e.rs`,
run as part of the workspace test suite) that exercises the toolkit's ledger-9 ECDSA
unshielded-signature support against a `dev` node (whose genesis is built on ledger 9, so the ECDSA
scheme is accepted on-chain).

Because every submitted transaction runs the ledger's `Transaction::well_formed` — i.e. the real
`signature_verify` — the test confirms end-to-end that:

- ECDSA unshielded address derivation is wired via `show-address --seed ecdsa:<seed>` and is
  distinct from the Schnorr address for the same seed (acceptance criterion #2).
- A contract can be deployed with an ECDSA contract-maintenance committee, and a maintenance update
  signed by that ECDSA committee is accepted (ECDSA-only signing, acceptance criterion #3).
- A maintenance update signed by a mixed Schnorr+ECDSA committee is accepted (per-member scheme
  dispatch in a single update), and authority rotations persist across sequential updates.

This complements the existing in-crate unit and ledger-acceptance tests with a full node round-trip.

PR: https://github.com/midnightntwrk/midnight-node/pull/1861
Issue: https://github.com/midnightntwrk/midnight-node/issues/1542
