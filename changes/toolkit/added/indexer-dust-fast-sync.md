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
(indexer 4.3.4 and later) or the older index range. Indexers before 4.3.4 report the tip's dust
roots for every block, so their snapshots rarely verify and those wallets fall back to the replay.

The spend-chain walk takes one `dustNullifierTransactions` round per spend in the longest chain, so
the wallet cache keeps its frontier: the snapshot block and every unspent DUST output there. The
next run still rebuilds both trees fresh at its tip, but resumes each chain from its cached output
and only walks the spends made since. A resume that fails (its block is no longer on the indexer's
chain, a cached output has no generation at the tip, a root mismatch) warns and retries a fresh fast
sync before falling back to the replay. Index-range indexers keep no frontier. The wallet cache
format moves to v6, so existing entries are re-synced once.

The new operations live in their own GraphQL documents. Indexers validate every operation in the
document they are sent, so an indexer without them still accepts the existing ones.

PR: <link to PR>
Issue: https://github.com/midnightntwrk/midnight-node/issues/1186
