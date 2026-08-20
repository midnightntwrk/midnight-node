-- Per-block dust commitment/generation Merkle tree roots (as of this block), so
-- `Block.dustCommitmentMerkleTreeRoot` / `dustGenerationMerkleTreeRoot` return
-- the root for the queried block rather than the latest indexed (tip) state.
-- The chain-indexer computes these from the post-apply ledger state in
-- `save_block` for new blocks.
--
-- Nullable, with no backfill: unlike the end-index columns these roots cannot be
-- derived in SQL (they require the ledger Merkle trees), so existing rows stay
-- NULL until the chain is reset or backfilled. The GraphQL fields are nullable
-- accordingly.

ALTER TABLE blocks ADD COLUMN dust_commitment_merkle_tree_root BYTEA;
ALTER TABLE blocks ADD COLUMN dust_generation_merkle_tree_root BYTEA;
