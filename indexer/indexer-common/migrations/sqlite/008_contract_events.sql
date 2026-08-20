-- Public Contract Events support (per MIP-107 / CoIP-442).
--
-- SQLite uses TEXT CHECK () constraints instead of native enums, so extending
-- the variant set requires rebuilding the ledger_events table. The data is
-- preserved by INSERT ... SELECT from the old table, then DROP + RENAME.
--
-- Also adds the contract_event_indexed_fields sidecar table for prefix
-- queries (mirror of dust_nullifiers shape in 001_initial.sql).

--------------------------------------------------------------------------------
-- Rebuild ledger_events with extended variant + grouping CHECK constraints
--------------------------------------------------------------------------------
CREATE TABLE ledger_events_new (
  id INTEGER PRIMARY KEY,
  transaction_id INTEGER NOT NULL REFERENCES transactions (id),
  variant TEXT CHECK (
    variant IN (
      'ZswapInput',
      'ZswapOutput',
      'ParamChange',
      'DustInitialUtxo',
      'DustGenerationDtimeUpdate',
      'DustSpendProcessed',
      'ShieldedSpend',
      'ShieldedReceive',
      'ShieldedMint',
      'ShieldedBurn',
      'UnshieldedSpend',
      'UnshieldedReceive',
      'UnshieldedMint',
      'UnshieldedBurn',
      'Paused',
      'Unpaused',
      'Misc'
    )
  ) NOT NULL,
  grouping TEXT CHECK (grouping IN ('Zswap', 'Dust', 'Contract')) NOT NULL,
  raw BYTEA NOT NULL,
  attributes TEXT NOT NULL,
  contract_address BLOB,
  contract_action_id INTEGER REFERENCES contract_actions (id)
);

INSERT INTO ledger_events_new (id, transaction_id, variant, grouping, raw, attributes)
SELECT id, transaction_id, variant, grouping, raw, attributes
FROM ledger_events;

DROP INDEX IF EXISTS ledger_events_transaction_id_idx;
DROP INDEX IF EXISTS ledger_events_variant_idx;
DROP INDEX IF EXISTS ledger_events_grouping_idx;
DROP INDEX IF EXISTS ledger_events_id_grouping_idx;
DROP INDEX IF EXISTS ledger_events_transaction_id_grouping_idx;
DROP TABLE ledger_events;
ALTER TABLE ledger_events_new RENAME TO ledger_events;

CREATE INDEX ledger_events_transaction_id_idx ON ledger_events (transaction_id);
CREATE INDEX ledger_events_variant_idx ON ledger_events (variant);
CREATE INDEX ledger_events_grouping_idx ON ledger_events (grouping);
CREATE INDEX ledger_events_id_grouping_idx ON ledger_events (id, grouping);
CREATE INDEX ledger_events_transaction_id_grouping_idx ON ledger_events (transaction_id, grouping);
CREATE INDEX ledger_events_contract_address_idx ON ledger_events (contract_address);
CREATE INDEX ledger_events_grouping_contract_address_idx ON ledger_events (grouping, contract_address);
CREATE INDEX ledger_events_contract_action_id_idx ON ledger_events (contract_action_id);

--------------------------------------------------------------------------------
-- contract_event_indexed_fields (sidecar)
--------------------------------------------------------------------------------
CREATE TABLE contract_event_indexed_fields (
  id INTEGER PRIMARY KEY,
  ledger_event_id INTEGER NOT NULL REFERENCES ledger_events (id),
  field_name TEXT NOT NULL,
  field_value BLOB NOT NULL
);
CREATE INDEX contract_event_indexed_fields_field_value_idx ON contract_event_indexed_fields (field_value);
CREATE INDEX contract_event_indexed_fields_ledger_event_id_idx ON contract_event_indexed_fields (ledger_event_id);
CREATE INDEX contract_event_indexed_fields_field_name_value_idx ON contract_event_indexed_fields (field_name, field_value);
