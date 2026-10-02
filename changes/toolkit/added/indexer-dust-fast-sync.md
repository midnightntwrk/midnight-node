#toolkit
# Fast DUST sync from an indexer snapshot

On the `--indexer-url` path, each wallet's DUST state is now rebuilt from per-wallet indexer
queries instead of replaying every `dustLedgerEvents` entry since genesis (187k on mainnet, 1.58M
on preprod as of October 2026):

1. `dustGenerations` returns the wallet's generation entries, with collapsed Merkle updates filling
   the generation tree's gaps.
2. `dustNullifierTransactions` follows each DUST output's spend chain, recomputing every successor
   output and checking it against the commitment the chain recorded.
3. `dustCommitmentMerkleTreeUpdate` fills the commitment tree around the unspent outputs.

The result is used only if both tree roots equal the tip block's `dustCommitmentMerkleTreeRoot` /
`dustGenerationMerkleTreeRoot`. Any failure (an indexer without these `@beta` operations, a root or
commitment mismatch, a stalled subscription) logs a warning and falls back to the event replay for
that wallet. `--no-fast-sync` (env `MN_INDEXER_NO_FAST_SYNC`) forces the replay.

The toolkit probes which `dustGenerations` signature the indexer serves: the block-pinned snapshot
(indexer 4.3.4 and later) or the older index range. Indexers before 4.3.4 report the tip's dust roots
for every block, so their snapshots rarely verify and those wallets fall back to the replay.

A fast-synced wallet has no dust event id to resume from, so the wallet cache stores no DUST state
for it and the next run fast-syncs again.

The new operations live in their own GraphQL documents. Indexers validate every operation in the
document they are sent, so an indexer without them still accepts the existing ones.

PR: <link to PR>
Issue: https://github.com/midnightntwrk/midnight-node/issues/1186
