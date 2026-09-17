#node #ledger
# Prepare batch proof verification incrementally, as transactions arrive

Aggregate proof verification has two halves: a per-proof preparation (transcript replay and a
deferred MSM guard, parallel across proofs) whose cost grows with the batch, and a fold plus a
single pairing check whose cost is essentially constant. Measured on this branch, one aggregate
call costs about `4.4 ms + 1.6 ms x n`, so at a batch of 16 roughly 86% of the work is the
per-proof half.

Preparation does not depend on which proofs end up sharing a batch — the batching challenge is
derived from the prepared guards only when they are folded — so it can be done as each transaction
arrives rather than when the batch is dispatched. The mempool already waits (for `k_target`
submissions, or up to `tau`), and that window was previously idle.

The split is now exposed end to end:

- `midnight-zk` gained `prepare_proofs` / `verify_prepared` around the existing private phases, and
  `batch_verify` is implemented in terms of them.
- `midnight-transient-crypto` gained `VerifierKey::prepare_batch` / `verify_prepared_batch`.
- `midnight-ledger` gained `ProofKind::prepare_proof_evidence` / `merge_prepared_evidence` /
  `verify_prepared_evidence`. Evidence with no such split — the legacy v2 proof batch, and every
  proof under mock verification — is carried through unprepared and verified during the fold.
- This crate gained `Bridge::prepare_transaction` / `finalize_prepared_batch`, reached natively via
  `host_api::ledger_9`, and the mempool dispatcher now prepares each submission on arrival and folds
  once at dispatch.

Block import is unchanged: every proof in a block arrives at once, so there is no waiting window to
overlap preparation with. Using this on the sync path would need lookahead across blocks, which
`sc_consensus::BasicQueue` does not allow — it verifies and imports strictly one block at a time.

PR: <link to PR>
