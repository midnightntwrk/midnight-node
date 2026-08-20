-- Add a UNIQUE index on `blocks.height`.
--
-- The postgres schema declares `height BIGINT NOT NULL UNIQUE`, which gives
-- postgres an implicit btree on height. The sqlite schema lacks that UNIQUE

CREATE UNIQUE INDEX IF NOT EXISTS blocks_height_idx ON blocks (height);
