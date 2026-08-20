-- Cardano-to-Midnight bridge events (PM-15404).
--
-- See the matching postgres/005_bridge_events.sql for full context. SQLite
-- has no ENUM type, so `variant` is a TEXT column with a CHECK constraint;
-- partial indexes use `WHERE` clauses.

--------------------------------------------------------------------------------
-- protocol_bridge_events
--------------------------------------------------------------------------------
CREATE TABLE protocol_bridge_events (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  block_id INTEGER NOT NULL REFERENCES blocks (id),
  transaction_id INTEGER REFERENCES transactions (id),
  variant TEXT NOT NULL CHECK (variant IN (
    'UserTransfer',
    'ReserveTransfer',
    'InvalidTransfer',
    'UnapprovedTransfer',
    'SubminimalFlushTransfer'
  )),
  mc_tx_hash BLOB,
  amount BLOB NOT NULL,
  recipient BLOB,
  midnight_tx_hash BLOB NOT NULL,
  count INTEGER
);

CREATE INDEX protocol_bridge_events_block_id_idx          ON protocol_bridge_events (block_id);
CREATE INDEX protocol_bridge_events_transaction_id_idx    ON protocol_bridge_events (transaction_id);
CREATE INDEX protocol_bridge_events_mc_tx_hash_idx        ON protocol_bridge_events (mc_tx_hash) WHERE mc_tx_hash IS NOT NULL;
CREATE INDEX protocol_bridge_events_midnight_tx_hash_idx  ON protocol_bridge_events (midnight_tx_hash);
CREATE INDEX protocol_bridge_events_recipient_idx         ON protocol_bridge_events (recipient) WHERE recipient IS NOT NULL;
CREATE INDEX protocol_bridge_events_variant_recipient_idx ON protocol_bridge_events (variant, recipient);

--------------------------------------------------------------------------------
-- bridge_claims
--------------------------------------------------------------------------------
CREATE TABLE bridge_claims (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  transaction_id INTEGER NOT NULL REFERENCES transactions (id),
  recipient BLOB NOT NULL,
  amount BLOB NOT NULL
);

CREATE INDEX bridge_claims_transaction_id_idx ON bridge_claims (transaction_id);
CREATE INDEX bridge_claims_recipient_idx      ON bridge_claims (recipient);
