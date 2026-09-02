-- Expose QualifiedDustOutput fields and the generation-tree index on
-- dust_generation_info.
--
-- Columns are added nullable so the migration runs cleanly on a populated
-- database. Rows inserted before this migration have NULL for the new
-- fields and are auto-skipped by the dustGenerations subscription's
-- `WHERE generation_index >= $cursor` clause (NULL fails the comparison).

--------------------------------------------------------------------------------
-- dust_generation_info
--------------------------------------------------------------------------------
ALTER TABLE dust_generation_info ADD COLUMN generation_index BIGINT;
ALTER TABLE dust_generation_info ADD COLUMN backing_night BYTEA;
ALTER TABLE dust_generation_info ADD COLUMN initial_value BYTEA;
CREATE INDEX ON dust_generation_info (owner, generation_index);
