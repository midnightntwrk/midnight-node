#node #performance
# Skip batch preparation for transactions the runtime has already validated

The transaction pool revalidates everything it still holds on every block import. On the inline
path those are answered by the ledger's soft cache on the first line of
`do_validate_transaction` — a `tx_hash` lookup that touches no proof. The batch ingress had no
such check: `Bridge::prepare_transaction` deserialized, ran `well_formed` and ran the full
per-proof preparation unconditionally, every time.

So with `batch_verify_mempool` enabled, verification cost scaled with **how long transactions sat
in the pool** rather than with how many were submitted. Holding a workload at 144 submissions and
varying only how long the node ran afterwards:

| settle after last submission | inline verifications | batch verifications (before) | after |
|---|---|---|---|
| 0 s | 144 | 350 | 144 |
| 45 s | 144 | 432 | 144 |

The inline path is pinned at exactly one verification per submission however long the node runs;
the batch path grew without bound. At 45 s that was 3.0x the verifications and 3.3x the
verification CPU (1.878 s against 0.574 s) for the same 144 transactions.

The batcher now takes the same short-circuit before preparing anything: if the runtime's soft
cache already holds a successful validation for the transaction, it delegates. Delegating rather
than synthesising a validity keeps the runtime authoritative and costs only the cache lookup it
was going to perform anyway. A new `midnight_batch_verify_soft_cache_short_circuits_total`
counter records how many submissions took that path — 573 of them in the 45 s run above.

This is the mechanism behind the redundant verification reported in the 2026-09-01 perfnet A/B
(60 transactions submitted, 150 verifications on the treatment arm; 67.7 s of verification CPU
against 36.5 s on control over a four-hour load). Those observations were correct and now have a
cause.

Fixing it does not make mempool batching a win — per verification it remains break-even on this
workload — but it removes a penalty that grew with congestion.

PR: <link to PR>
