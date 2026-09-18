#node #performance
# Verify block proofs ahead of the import cursor

`BasicQueue` imports blocks strictly sequentially — `import_many_blocks` awaits
each block's `import_block` before starting the next — and the batch verifier
runs *inside* that call. A syncing node therefore pays `verify(N) + execute(N)`
per block, although the two are independent.

A new `LookaheadImportQueue` wraps the queue and records each downloaded chunk
without verifying it. Then, as each block finishes importing, its state becomes
the reference for the next group of queued blocks, which is dispatched to a
blocking worker pool. By the time the sequential importer reaches those blocks
their proofs are usually already verified, and the import path consumes the
result instead of doing the work.

Off by default; `batch_verify_lookahead` enables it alongside
`batch_verify_block_import`, with `batch_verify_lookahead_blocks` (group size)
and `batch_verify_lookahead_workers`.

Measured over a 129-block, 518-transaction sync, 15 paired runs: the syncing
node was faster in 13 of 15, median +0.50s, mean +0.58s (sign test p = 0.004).
That matches the mechanism — verification serialized on the import path takes
1.462s, while with lookahead imports waited only 0.904s for results, so ~0.56s
moved off the critical path. Total verification work does not go down; it stops
being in the way.

## Scheduling from the import cursor, not from ingress

The obvious design — verify a chunk when the sync engine queues it — does not
work, and fails silently. The sync engine downloads well ahead of the import
cursor, so the parents of the queued blocks are themselves still queued and
their states do not exist. Verifying against the node's current best instead
makes almost every transaction fail the *non-crypto* `well_formed` checks; the
aggregate call then "succeeds" having verified nothing, the import path skips
verification that never happened, and every transaction is re-verified inline at
full per-transaction cost. Measured, that variant ran 0.93x — slower than no
lookahead at all, while also burning worker CPU.

Two guards come from that:

- A group is only reported verified when *every* transaction in it verified. An
  outer `Ok` from the batch entry point only means the aggregate call completed;
  transactions that failed the non-crypto checks come back as inner `Err`s.
- `max_stale_blocks` bounds how far past its reference a group may reach.

## Why a stale reference cannot reject a valid block

Jobs run with `isolate_on_failure = false`, so an aggregate failure returns
before anything is written to the revalidation cache, and a transaction that
fails the non-crypto checks is dropped from the batch without a cache write
either. A lookahead can only ever record `VerifiedAt`, never `Invalid`, and
`get_verified_transaction` re-checks that record against the real state through
the ledger's `RevalidationReference` before trusting it. The worst a stale
reference does is cost the optimisation.

PR: <link to PR>
