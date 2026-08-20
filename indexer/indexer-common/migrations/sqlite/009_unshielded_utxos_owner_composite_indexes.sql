-- Owner-leading composites for queries that filter by owner equality and a
-- tx_id range. See PG migration 007 for the access pattern. SQLite had no
-- composite indexes on unshielded_utxos prior to this migration.

--------------------------------------------------------------------------------
-- unshielded_utxos
--------------------------------------------------------------------------------
CREATE INDEX unshielded_owner_creating_idx ON unshielded_utxos (owner, creating_transaction_id);
CREATE INDEX unshielded_owner_spending_idx ON unshielded_utxos (owner, spending_transaction_id);
