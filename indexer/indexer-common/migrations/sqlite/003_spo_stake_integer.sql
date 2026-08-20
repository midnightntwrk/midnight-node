-- Migrate lovelace columns on the SPO stake tables from REAL to INTEGER.
--
-- SQLite REAL is IEEE-754 f64, which only represents integers exactly up to
-- 2^53 (≈ 9.0 × 10^15). Cardano total supply is 4.5 × 10^16 lovelace and
-- aggregate SUM queries over per-pool stakes can exceed 2^53, so keeping these
-- columns as REAL would silently lose precision. INTEGER in SQLite is 64-bit
-- signed (up to ≈ 9.2 × 10^18), which fits total supply comfortably.
--
-- SQLite cannot change a column's type in place, so rebuild each table.
-- Individual per-pool stakes are below 2^53 and already stored exactly as
-- REAL, so CAST(x AS INTEGER) preserves the value.

--------------------------------------------------------------------------------
-- spo_stake_snapshot
--------------------------------------------------------------------------------
CREATE TABLE spo_stake_snapshot_new (
  pool_id TEXT PRIMARY KEY REFERENCES pool_metadata_cache (pool_id) ON DELETE CASCADE,
  live_stake INTEGER,
  active_stake INTEGER,
  live_delegators INTEGER,
  live_saturation REAL,
  declared_pledge INTEGER,
  live_pledge INTEGER,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

INSERT INTO spo_stake_snapshot_new (
  pool_id, live_stake, active_stake, live_delegators, live_saturation,
  declared_pledge, live_pledge, updated_at
)
SELECT pool_id,
       CAST(live_stake AS INTEGER),
       CAST(active_stake AS INTEGER),
       live_delegators,
       live_saturation,
       CAST(declared_pledge AS INTEGER),
       CAST(live_pledge AS INTEGER),
       updated_at
FROM spo_stake_snapshot;

DROP INDEX IF EXISTS spo_stake_snapshot_updated_at_idx;
DROP INDEX IF EXISTS spo_stake_snapshot_live_stake_idx;
DROP TABLE spo_stake_snapshot;
ALTER TABLE spo_stake_snapshot_new RENAME TO spo_stake_snapshot;

CREATE INDEX spo_stake_snapshot_updated_at_idx ON spo_stake_snapshot (updated_at DESC);
CREATE INDEX spo_stake_snapshot_live_stake_idx ON spo_stake_snapshot (COALESCE(live_stake, 0) DESC);

--------------------------------------------------------------------------------
-- spo_stake_history
--------------------------------------------------------------------------------
CREATE TABLE spo_stake_history_new (
  id INTEGER PRIMARY KEY,
  pool_id TEXT NOT NULL REFERENCES pool_metadata_cache (pool_id) ON DELETE CASCADE,
  recorded_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  mainchain_epoch INTEGER,
  live_stake INTEGER,
  active_stake INTEGER,
  live_delegators INTEGER,
  live_saturation REAL,
  declared_pledge INTEGER,
  live_pledge INTEGER
);

INSERT INTO spo_stake_history_new (
  id, pool_id, recorded_at, mainchain_epoch,
  live_stake, active_stake, live_delegators, live_saturation,
  declared_pledge, live_pledge
)
SELECT id, pool_id, recorded_at, mainchain_epoch,
       CAST(live_stake AS INTEGER),
       CAST(active_stake AS INTEGER),
       live_delegators,
       live_saturation,
       CAST(declared_pledge AS INTEGER),
       CAST(live_pledge AS INTEGER)
FROM spo_stake_history;

DROP INDEX IF EXISTS spo_stake_history_pool_time_idx;
DROP INDEX IF EXISTS spo_stake_history_epoch_idx;
DROP TABLE spo_stake_history;
ALTER TABLE spo_stake_history_new RENAME TO spo_stake_history;

CREATE INDEX spo_stake_history_pool_time_idx ON spo_stake_history (pool_id, recorded_at DESC);
CREATE INDEX spo_stake_history_epoch_idx ON spo_stake_history (mainchain_epoch);
