#node #ledger #performance
# Revalidation skips the proof cryptography, and only that

"Verify a transaction's ZK proofs once, not once per state it is validated against"
described `RevalidationReference` as skipping "proof, signature and
binding-commitment verification" via `StateReference::stateless_check`. That was
wrong. `stateless_check` guards only the signature, binding-commitment and zswap
structural checks; proof verification is gated separately, by
`WellFormedStrictness`. The revalidation path therefore re-ran the full proof
cryptography, and the reported 1.00x came from counting `mode="inline"` metric
samples — which the revalidation branch stops emitting while the work they name
carries on.

The obvious fix, passing `defer_proofs()`, is unsound: in the ledger, `op_check`
and `dust_spend_check` are reachable only under `verify_contract_proofs` /
`verify_native_proofs`, directly or through `collect_proof_evidence`. Deferring
the proofs skips exactly the state-dependent re-checks that make reusing an
earlier verification safe, so a transaction would still be accepted after its
contract's verifier key changed, or after pruning moved the Dust root at its
ctime.

The ledger now provides `StateReference::adjust_strictness`, and
`RevalidationReference` overrides it to apply
`WellFormedStrictness::assume_proofs_verified`. Proof verification was the one
check with no hook on `StateReference`, which is why a reference alone could not
stand it down; with the hook the reference states its own policy and this call
site simply passes the caller's strictness through. Evidence collection still
runs, so the state-dependent checks still run, and the verifier key is still
resolved.

Measured over an 82-block, 233-transaction sync:

| per transaction | before | after |
|---|---|---|
| revalidation `well_formed` | 3.169 ms | 0.511 ms |

| whole sync | OFF (inline) | ON (batch) |
|---|---|---|
| total proof verification | 0.895s | 0.612s |

That turns batch verification on block import from a net loss (0.73x) into a
1.46x win. End-to-end sync time improves accordingly: over 21 paired runs the
batching node was faster in 17, median +0.20s, mean +0.29s (sign test p = 0.004).
The mean matches the 0.283s of verification saved, so the wall-clock gain is
fully accounted for by the measured work removed.

A new `ledger_proof_verify_duration_seconds{mode="revalidate"}` metric records
this path. Without it the regression was invisible: the crypto-only comparison
showed batch verification 2.03x faster while end-to-end sync was consistently
slower, because the cost had moved into a pass nothing measured.

PR: <link to PR>
