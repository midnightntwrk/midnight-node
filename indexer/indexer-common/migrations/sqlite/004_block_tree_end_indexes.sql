-- Per-block tree end indexes (chain's `*_first_free` as of this block).
-- Lets clients read a chain-aligned upper bound for `dustGenerations` and
-- the zswap / dust-commitment merkle update queries directly from `Block`,
-- without walking back through transactions to find one of variant
-- `RegularTransaction` (the only variant that carries these on the
-- transaction level today).
--
-- Columns are added with `NOT NULL DEFAULT 0` so the ALTER runs cleanly on a
-- populated database. The backfill below derives historic values from the
-- highest `RegularTransaction.*_end_index` seen up to and including each
-- block (running max ordered by `height`). New blocks get the accurate
-- ledger-state value via the chain-indexer's `save_block` write path.
--
-- Approximation note: for blocks whose `SystemTransaction`s also modify
-- these trees (notably the genesis block 0 with initial dust generation
-- seeding), the backfilled value lags the true chain state since the
-- `*_end_index` columns only live on `regular_transactions`. New blocks
-- always carry the exact ledger-state value.

ALTER TABLE blocks ADD COLUMN zswap_end_index INTEGER NOT NULL DEFAULT 0;
ALTER TABLE blocks ADD COLUMN dust_commitment_end_index INTEGER NOT NULL DEFAULT 0;
ALTER TABLE blocks ADD COLUMN dust_generation_end_index INTEGER NOT NULL DEFAULT 0;

WITH per_block_max AS (
    SELECT
        b.id AS block_id,
        b.height AS block_height,
        MAX(rt.zswap_end_index) AS zswap_max,
        MAX(rt.dust_commitment_end_index) AS dust_commitment_max,
        MAX(rt.dust_generation_end_index) AS dust_generation_max
    FROM blocks b
    LEFT JOIN transactions t ON t.block_id = b.id
    LEFT JOIN regular_transactions rt ON rt.id = t.id
    GROUP BY b.id, b.height
),
running_max AS (
    SELECT
        block_id,
        COALESCE(MAX(zswap_max) OVER (ORDER BY block_height ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW), 0) AS zswap_end_index,
        COALESCE(MAX(dust_commitment_max) OVER (ORDER BY block_height ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW), 0) AS dust_commitment_end_index,
        COALESCE(MAX(dust_generation_max) OVER (ORDER BY block_height ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW), 0) AS dust_generation_end_index
    FROM per_block_max
)
UPDATE blocks
SET zswap_end_index = running_max.zswap_end_index,
    dust_commitment_end_index = running_max.dust_commitment_end_index,
    dust_generation_end_index = running_max.dust_generation_end_index
FROM running_max
WHERE blocks.id = running_max.block_id;
