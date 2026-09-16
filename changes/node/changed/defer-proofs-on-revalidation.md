#node #ledger #performance
# Actually skip the ZK proofs when revalidating a transaction

The revalidation path added in "Verify a transaction's ZK proofs once, not once
per state it is validated against" did not skip proof verification. That change
described `RevalidationReference` as skipping "proof, signature and
binding-commitment verification" via `StateReference::stateless_check`, but
`stateless_check` only guards the signature, binding-commitment and zswap
structural checks. Proof verification is gated separately, by
`WellFormedStrictness` — and the call site passed `WellFormedStrictness::default()`,
which has `verify_native_proofs` and `verify_contract_proofs` both set.

So every revalidation re-ran the entire proof crypto. The earlier 1.00x figure was
measured by counting `mode="inline"` metric samples, which did drop to one per
transaction because the revalidation branch stops emitting that sample — while the
work it names carried on happening. The revalidation path is now passed
`defer_proofs()`, which is sound for the reason the reference exists: the
revalidation cache is keyed by the transaction's SHA-256 `transaction_hash`, so the
proof bytes and every stateless public input are identical to a transaction whose
proofs already verified, and the state-dependent inputs are exactly what
`RevalidationReference` re-checks.

Measured on an 82-block, 233-transaction sync, per transaction:

| | before | after |
|---|---|---|
| revalidation `well_formed` | 3.169 ms | 0.172 ms |

and over the whole sync, total proof-verification cost with batch verification on
fell from 1.236s to 0.513s against an inline baseline of 0.884s — from 0.73x
(batching cost *more* than it saved) to 1.72x.

A new `ledger_proof_verify_duration_seconds{mode="revalidate"}` metric records this
path, so the revalidation cost is observable rather than inferred. Without it the
regression was invisible: the crypto-only comparison showed batch verification
2.07x faster while end-to-end sync was consistently slower, because the cost had
moved into a pass nothing measured.

PR: <link to PR>
