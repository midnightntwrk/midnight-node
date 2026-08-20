--------------------------------------------------------------------------------
-- ledger_db_nodes
--------------------------------------------------------------------------------
CREATE TABLE ledger_db_nodes (key BLOB PRIMARY KEY, object BLOB NOT NULL);
--------------------------------------------------------------------------------
-- ledger_db_roots
--------------------------------------------------------------------------------
CREATE TABLE ledger_db_roots (key BLOB PRIMARY KEY, count INTEGER NOT NULL);
CREATE INDEX ledger_db_roots_count_idx ON ledger_db_roots (count);
