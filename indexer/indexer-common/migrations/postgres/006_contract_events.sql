-- Public Contract Events support (per MIP-107 / CoIP-442).
--
-- Extends LEDGER_EVENT_GROUPING with `Contract` and LEDGER_EVENT_VARIANT with
-- 11 contract event variants (one per LogEventType in onchain-vm/src/ops.rs at
-- onchain-runtime-4.0.0-alpha.1). Adds the contract_event_indexed_fields
-- sidecar table for prefix-filterable indexed-field queries.
--
-- Standard event types follow CoIP-442 Appendix A head; Paused/Unpaused are
-- signal-only (no indexed fields), Misc is opaque.

--------------------------------------------------------------------------------
-- enum extensions
--------------------------------------------------------------------------------
ALTER TYPE LEDGER_EVENT_GROUPING ADD VALUE 'Contract';

ALTER TYPE LEDGER_EVENT_VARIANT ADD VALUE 'ShieldedSpend';
ALTER TYPE LEDGER_EVENT_VARIANT ADD VALUE 'ShieldedReceive';
ALTER TYPE LEDGER_EVENT_VARIANT ADD VALUE 'ShieldedMint';
ALTER TYPE LEDGER_EVENT_VARIANT ADD VALUE 'ShieldedBurn';
ALTER TYPE LEDGER_EVENT_VARIANT ADD VALUE 'UnshieldedSpend';
ALTER TYPE LEDGER_EVENT_VARIANT ADD VALUE 'UnshieldedReceive';
ALTER TYPE LEDGER_EVENT_VARIANT ADD VALUE 'UnshieldedMint';
ALTER TYPE LEDGER_EVENT_VARIANT ADD VALUE 'UnshieldedBurn';
ALTER TYPE LEDGER_EVENT_VARIANT ADD VALUE 'Paused';
ALTER TYPE LEDGER_EVENT_VARIANT ADD VALUE 'Unpaused';
ALTER TYPE LEDGER_EVENT_VARIANT ADD VALUE 'Misc';

--------------------------------------------------------------------------------
-- contract_address on ledger_events
--
-- Nullable to preserve existing zswap/dust rows. For contract events, the
-- column carries the emitting contract address (ContractLog.address) and is
-- indexed for fast filtering on the contractEvents query/subscription.
--------------------------------------------------------------------------------
ALTER TABLE ledger_events ADD COLUMN contract_address BYTEA;
CREATE INDEX ON ledger_events (contract_address);
CREATE INDEX ON ledger_events (grouping, contract_address);

--------------------------------------------------------------------------------
-- contract_action_id on ledger_events
--
-- Nullable to preserve existing zswap/dust rows. For contract events, the
-- column links the event to the specific ContractCall (contract_actions.id)
-- that emitted it. Powers the nested ContractCall.contractEvents GraphQL
-- field (#1162) via DataLoader batching.
--
-- Chain-indexer populates this when the v9 parser writes contract events
-- (gated on #1157). Pre-#1157 contract events do not flow at all so this
-- column being NULL doesn't impact correctness today.
--------------------------------------------------------------------------------
ALTER TABLE ledger_events
  ADD COLUMN contract_action_id BIGINT REFERENCES contract_actions (id);
CREATE INDEX ON ledger_events (contract_action_id);

--------------------------------------------------------------------------------
-- contract_event_indexed_fields (sidecar)
--
-- Per-row indexed field values for standard contract events, supporting
-- prefix-match queries from the contractEvents query/subscription surface.
-- Shape mirrors dust_nullifiers / zswap_nullifiers (BYTEA values, B-tree
-- index for prefix lookup via LIKE).
--
-- field_name identifies which event field this row represents
-- (e.g. 'nullifier', 'commitment', 'sender', 'tokenType'), the indexer
-- enforces per-event-variant field name validity at the resolver layer.
--------------------------------------------------------------------------------
CREATE TABLE contract_event_indexed_fields (
  id BIGSERIAL PRIMARY KEY,
  ledger_event_id BIGINT NOT NULL REFERENCES ledger_events (id),
  field_name TEXT NOT NULL,
  field_value BYTEA NOT NULL
);
CREATE INDEX ON contract_event_indexed_fields (field_value);
CREATE INDEX ON contract_event_indexed_fields (ledger_event_id);
CREATE INDEX ON contract_event_indexed_fields (field_name, field_value);
