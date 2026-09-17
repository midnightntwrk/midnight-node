#fix #tooling
# Fix reserve-contracts CLI governance-update txs on local-env (Ogmios 3002)

The bun CLI's governance-update commands (e.g. `change-federated-ops`) failed
with Ogmios error 3002 ("additional UTxO entries overlap with those that exist
in the ledger") when spending existing on-chain Plutus script UTxOs via the
kupmios provider: Blaze passes the tx's attached on-chain inputs to Ogmios
`evaluateTransaction` as `additionalUtxo`, which Ogmios rejects. `complete-tx.ts`
now reuses the local UPLC evaluator (already run in phase 1) for phase-2
completion, avoiding the provider evaluate round-trip — guarded to only apply
when the local eval succeeded, so deploy-time commands are unchanged. Bumps the
`midnight-reserve-contracts` submodule.

PR: https://github.com/midnightntwrk/midnight-node/pull/1934
Issue: https://github.com/midnightntwrk/midnight-node/issues/1933
