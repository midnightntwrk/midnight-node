#node #ledger
# Correct the claim that revalidation skips proof verification

"Verify a transaction's ZK proofs once, not once per state it is validated
against" described `RevalidationReference` as skipping "proof, signature and
binding-commitment verification" via `StateReference::stateless_check`. That is
wrong. `stateless_check` guards only the signature, binding-commitment and zswap
structural checks. Proof verification is gated separately, by the
`verify_native_proofs` / `verify_contract_proofs` flags on
`WellFormedStrictness` — so the revalidation path has always re-run the full
proof crypto, and the reported 1.00x was measured by counting `mode="inline"`
metric samples, which stop being emitted on that path while the work they name
carries on.

Nor can the node simply pass `defer_proofs()`. In the ledger, the state-dependent
checks that make reusing a previous verification safe are nested *inside* the
proof flags: `op_check` (verify.rs) and `dust_spend_check` (dust.rs) are each
reachable only under `verify_contract_proofs` / `verify_native_proofs`, directly
or through `collect_proof_evidence`. Deferring the proofs would skip precisely
the re-checks the reference exists to perform — the contract's registered
operation and verifier key, and the Dust roots at the transaction's ctime — and
so accept a transaction whose proofs no longer hold against the new state.

What revalidation actually buys today is the signature, binding-commitment and
zswap structural work: measured at 3.17 ms/tx against 3.92 ms/tx for a full
inline verification. The call site keeps the default strictness and now says why.

Consequence for batch verification on block import: it is a net loss on the
measured chain. Aggregate verification makes the crypto 2.07x cheaper, but the
ON path also pays a revalidation pass during execution that the OFF path does
not, and the totals over an 82-block, 233-transaction sync are 0.513s of batch +
prep versus 0.884s inline, plus 0.738s of revalidation — 1.24s against 0.88s, a
ratio of 0.73x.

Making it a win needs a ledger-side way to run evidence collection and its
state-dependent checks while skipping only the cryptographic verification — for
example a `ProofVerificationMode` variant that checks public inputs and returns
without verifying, letting a caller keep `verify_*_proofs` set. With that, the
revalidation pass should cost roughly the non-crypto `well_formed` plus evidence
collection, and batch verification would come out ahead on block import.

PR: <link to PR>
