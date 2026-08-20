--------------------------------------------------------------------------------
-- types
--------------------------------------------------------------------------------
CREATE TYPE CONTRACT_ACTION_VARIANT AS ENUM('Deploy', 'Call', 'Update');
CREATE TYPE LEDGER_EVENT_GROUPING AS ENUM('Zswap', 'Dust');
CREATE TYPE LEDGER_EVENT_VARIANT AS ENUM(
  'ZswapInput',
  'ZswapOutput',
  'ParamChange',
  'DustInitialUtxo',
  'DustGenerationDtimeUpdate',
  'DustSpendProcessed'
);
CREATE TYPE TRANSACTION_VARIANT AS ENUM('Regular', 'System');
--------------------------------------------------------------------------------
-- blocks
--------------------------------------------------------------------------------
CREATE TABLE blocks (
  id BIGSERIAL PRIMARY KEY,
  hash BYTEA NOT NULL UNIQUE,
  height BIGINT NOT NULL UNIQUE,
  protocol_version BIGINT NOT NULL,
  parent_hash BYTEA NOT NULL,
  author BYTEA,
  timestamp BIGINT NOT NULL,
  zswap_merkle_tree_root BYTEA NOT NULL,
  ledger_parameters BYTEA NOT NULL,
  ledger_state_key BYTEA NOT NULL
);
--------------------------------------------------------------------------------
-- transactions
--------------------------------------------------------------------------------
CREATE TABLE transactions (
  id BIGSERIAL PRIMARY KEY,
  block_id BIGINT NOT NULL REFERENCES blocks (id),
  variant TRANSACTION_VARIANT NOT NULL,
  hash BYTEA NOT NULL,
  protocol_version BIGINT NOT NULL,
  raw BYTEA NOT NULL
);
CREATE INDEX ON transactions (block_id);
CREATE INDEX ON transactions (hash);
CREATE INDEX ON transactions (variant, id);
--------------------------------------------------------------------------------
-- regular_transactions
--------------------------------------------------------------------------------
CREATE TABLE regular_transactions (
  id BIGINT PRIMARY KEY REFERENCES transactions (id),
  transaction_result JSONB NOT NULL,
  zswap_merkle_tree_root BYTEA NOT NULL,
  zswap_start_index BIGINT NOT NULL,
  zswap_end_index BIGINT NOT NULL,
  dust_commitment_start_index BIGINT NOT NULL,
  dust_commitment_end_index BIGINT NOT NULL,
  dust_generation_start_index BIGINT NOT NULL,
  dust_generation_end_index BIGINT NOT NULL,
  paid_fees BYTEA,
  estimated_fees BYTEA,
  identifiers BYTEA[] NOT NULL
);
CREATE INDEX ON regular_transactions (transaction_result);
CREATE INDEX ON regular_transactions USING GIN (transaction_result);
CREATE INDEX ON regular_transactions (zswap_start_index);
CREATE INDEX ON regular_transactions (zswap_end_index);
--------------------------------------------------------------------------------
-- contract_actions
--------------------------------------------------------------------------------
CREATE TABLE contract_actions (
  id BIGSERIAL PRIMARY KEY,
  transaction_id BIGINT NOT NULL REFERENCES transactions (id),
  variant CONTRACT_ACTION_VARIANT NOT NULL,
  address BYTEA NOT NULL,
  state BYTEA NOT NULL,
  zswap_state BYTEA NOT NULL,
  attributes JSONB NOT NULL
);
CREATE INDEX ON contract_actions (transaction_id);
CREATE INDEX ON contract_actions (address);
CREATE INDEX ON contract_actions (id, address);
--------------------------------------------------------------------------------
-- unshielded_utxos
--------------------------------------------------------------------------------
CREATE TABLE unshielded_utxos (
  id BIGSERIAL PRIMARY KEY,
  creating_transaction_id BIGINT NOT NULL REFERENCES transactions (id),
  spending_transaction_id BIGINT REFERENCES transactions (id),
  owner BYTEA NOT NULL,
  token_type BYTEA NOT NULL,
  value BYTEA NOT NULL,
  intent_hash BYTEA NOT NULL,
  output_index BIGINT NOT NULL,
  ctime BIGINT,
  initial_nonce BYTEA NOT NULL,
  registered_for_dust_generation BOOLEAN NOT NULL,
  UNIQUE (intent_hash, output_index)
);
CREATE INDEX ON unshielded_utxos (creating_transaction_id);
CREATE INDEX ON unshielded_utxos (spending_transaction_id);
CREATE INDEX ON unshielded_utxos (owner);
CREATE INDEX ON unshielded_utxos (creating_transaction_id, owner);
CREATE INDEX ON unshielded_utxos (spending_transaction_id, owner);
CREATE INDEX ON unshielded_utxos (token_type);
--------------------------------------------------------------------------------
-- ledger_events
--------------------------------------------------------------------------------
CREATE TABLE ledger_events (
  id BIGSERIAL PRIMARY KEY,
  transaction_id BIGINT NOT NULL REFERENCES transactions (id),
  variant LEDGER_EVENT_VARIANT NOT NULL,
  grouping LEDGER_EVENT_GROUPING NOT NULL,
  raw BYTEA NOT NULL,
  attributes JSONB NOT NULL
);
CREATE INDEX ON ledger_events (transaction_id);
CREATE INDEX ON ledger_events (variant);
CREATE INDEX ON ledger_events (grouping);
CREATE INDEX ON ledger_events (id, grouping);
CREATE INDEX ON ledger_events (transaction_id, grouping);
--------------------------------------------------------------------------------
-- contract_balances
--------------------------------------------------------------------------------
CREATE TABLE contract_balances (
  id BIGSERIAL PRIMARY KEY,
  contract_action_id BIGINT NOT NULL REFERENCES contract_actions (id),
  token_type BYTEA NOT NULL, -- Serialized TokenType (hex-encoded)
  amount BYTEA NOT NULL, -- u128 amount as bytes (for large number support)
  UNIQUE (contract_action_id, token_type)
);
CREATE INDEX ON contract_balances (contract_action_id);
CREATE INDEX ON contract_balances (token_type);
CREATE INDEX ON contract_balances (contract_action_id, token_type);
--------------------------------------------------------------------------------
-- wallets
--------------------------------------------------------------------------------
CREATE TABLE wallets (
  id UUID PRIMARY KEY,
  viewing_key_hash BYTEA NOT NULL UNIQUE,
  viewing_key BYTEA NOT NULL, -- Ciphertext with nonce, no longer unique!
  wanted_start_index BIGINT NOT NULL DEFAULT 0,
  first_indexed_transaction_id BIGINT NOT NULL DEFAULT 0,
  last_indexed_transaction_id BIGINT NOT NULL DEFAULT 0,
  last_active TIMESTAMPTZ NOT NULL,
  session_id BYTEA UNIQUE -- Random per-session ID for API authentication, NULL when disconnected.
);
CREATE INDEX ON wallets (viewing_key_hash);
CREATE INDEX ON wallets (last_indexed_transaction_id DESC);
--------------------------------------------------------------------------------
-- relevant_transactions
--------------------------------------------------------------------------------
CREATE TABLE relevant_transactions (
  id BIGSERIAL PRIMARY KEY,
  wallet_id UUID NOT NULL REFERENCES wallets (id),
  transaction_id BIGINT NOT NULL REFERENCES transactions (id),
  UNIQUE (wallet_id, transaction_id)
);
--------------------------------------------------------------------------------
-- DUST Generation Status Tables
-- These tables support the dustGenerationStatus GraphQL query for Protofire dApp
--------------------------------------------------------------------------------
-- DUST generation information tracking
CREATE TABLE dust_generation_info (
  id BIGSERIAL PRIMARY KEY,
  night_utxo_hash BYTEA NOT NULL,
  value BYTEA NOT NULL,
  owner BYTEA NOT NULL,
  nonce BYTEA NOT NULL,
  ctime BIGINT NOT NULL,
  merkle_index BIGINT NOT NULL,
  dtime BIGINT,
  transaction_id BIGINT REFERENCES transactions (id)
);
CREATE INDEX ON dust_generation_info (owner);
CREATE INDEX ON dust_generation_info (night_utxo_hash);
CREATE INDEX ON dust_generation_info (transaction_id);
CREATE TABLE dust_nullifiers (
  id BIGSERIAL PRIMARY KEY,
  nullifier BYTEA NOT NULL,
  commitment BYTEA NOT NULL,
  transaction_id BIGINT NOT NULL REFERENCES transactions (id),
  block_id BIGINT NOT NULL REFERENCES blocks (id)
);
CREATE INDEX ON dust_nullifiers (nullifier);
CREATE INDEX ON dust_nullifiers (transaction_id);
CREATE INDEX ON dust_nullifiers (block_id);
-- Zswap (shielded) nullifier tracking for shieldedNullifierTransactions subscription
CREATE TABLE zswap_nullifiers (
  id BIGSERIAL PRIMARY KEY,
  transaction_id BIGINT NOT NULL REFERENCES transactions (id),
  block_id BIGINT NOT NULL REFERENCES blocks (id),
  nullifier BYTEA NOT NULL
);
CREATE INDEX ON zswap_nullifiers (nullifier);
CREATE INDEX ON zswap_nullifiers (transaction_id);
CREATE INDEX ON zswap_nullifiers (block_id);
-- cNIGHT registration tracking
CREATE TABLE cnight_registrations (
  id BIGSERIAL PRIMARY KEY,
  cardano_stake_key BYTEA NOT NULL,
  dust_address BYTEA NOT NULL,
  valid BOOLEAN NOT NULL,
  registered_at BIGINT NOT NULL,
  removed_at BIGINT,
  block_id BIGINT REFERENCES blocks (id),
  utxo_tx_hash BYTEA,
  utxo_output_index BIGINT,
  UNIQUE (cardano_stake_key, dust_address)
);
CREATE INDEX ON cnight_registrations (cardano_stake_key);
CREATE INDEX ON cnight_registrations (dust_address);
CREATE INDEX ON cnight_registrations (block_id);
CREATE TABLE system_parameters_terms_and_conditions (
  id BIGSERIAL PRIMARY KEY,
  block_height BIGINT NOT NULL,
  block_hash BYTEA NOT NULL,
  timestamp BIGINT NOT NULL,
  hash BYTEA NOT NULL,
  url TEXT NOT NULL
);
CREATE INDEX ON system_parameters_terms_and_conditions (block_height DESC);
CREATE TABLE system_parameters_d (
  id BIGSERIAL PRIMARY KEY,
  block_height BIGINT NOT NULL,
  block_hash BYTEA NOT NULL,
  timestamp BIGINT NOT NULL,
  num_permissioned_candidates INTEGER NOT NULL,
  num_registered_candidates INTEGER NOT NULL
);
CREATE INDEX ON system_parameters_d (block_height DESC);
--------------------------------------------------------------------------------
-- epochs
--------------------------------------------------------------------------------
CREATE TABLE epochs (
  epoch_no BIGINT PRIMARY KEY,
  starts_at TIMESTAMPTZ NOT NULL,
  ends_at TIMESTAMPTZ NOT NULL
);
--------------------------------------------------------------------------------
-- pool_metadata_cache
--------------------------------------------------------------------------------
CREATE TABLE pool_metadata_cache (
  pool_id VARCHAR PRIMARY KEY,
  hex_id VARCHAR UNIQUE,
  name TEXT,
  ticker TEXT,
  homepage_url TEXT,
  updated_at TIMESTAMPTZ,
  url TEXT
);
--------------------------------------------------------------------------------
-- spo_identity
--------------------------------------------------------------------------------
CREATE TABLE spo_identity (
  spo_sk VARCHAR PRIMARY KEY,
  sidechain_pubkey VARCHAR UNIQUE,
  pool_id VARCHAR REFERENCES pool_metadata_cache (pool_id),
  mainchain_pubkey VARCHAR UNIQUE,
  aura_pubkey VARCHAR UNIQUE
);
CREATE INDEX IF NOT EXISTS spo_identity_pk ON spo_identity (pool_id, sidechain_pubkey, aura_pubkey);
--------------------------------------------------------------------------------
-- committee_membership
--------------------------------------------------------------------------------
CREATE TABLE committee_membership (
  spo_sk VARCHAR,
  sidechain_pubkey VARCHAR,
  epoch_no BIGINT NOT NULL,
  position INT NOT NULL,
  expected_slots INT NOT NULL,
  PRIMARY KEY (epoch_no, position)
);
CREATE INDEX IF NOT EXISTS committee_membership_epoch_no_idx ON committee_membership (epoch_no);
--------------------------------------------------------------------------------
-- spo_epoch_performance
--------------------------------------------------------------------------------
CREATE TABLE spo_epoch_performance (
  spo_sk VARCHAR REFERENCES spo_identity (spo_sk),
  identity_label VARCHAR,
  epoch_no BIGINT NOT NULL,
  expected_blocks INT NOT NULL,
  produced_blocks INT NOT NULL,
  PRIMARY KEY (epoch_no, spo_sk)
);
CREATE INDEX IF NOT EXISTS spo_epoch_performance_identity_pk ON spo_epoch_performance (epoch_no, identity_label);
CREATE INDEX IF NOT EXISTS spo_epoch_performance_epoch_no_idx ON spo_epoch_performance (epoch_no);
--------------------------------------------------------------------------------
-- spo_history
--------------------------------------------------------------------------------
CREATE TABLE spo_history (
  spo_hist_sk BIGSERIAL PRIMARY KEY,
  spo_sk VARCHAR REFERENCES spo_identity (spo_sk),
  epoch_no BIGINT NOT NULL,
  status TEXT NOT NULL,
  valid_from BIGINT NOT NULL,
  valid_to BIGINT NOT NULL,
  UNIQUE (spo_sk, epoch_no)
);
CREATE INDEX IF NOT EXISTS spo_history_epoch_no_idx ON spo_history (epoch_no);
--------------------------------------------------------------------------------
-- spo_stake_snapshot
--------------------------------------------------------------------------------
CREATE TABLE spo_stake_snapshot (
  pool_id VARCHAR PRIMARY KEY REFERENCES pool_metadata_cache (pool_id) ON DELETE CASCADE,
  live_stake NUMERIC,
  active_stake NUMERIC,
  live_delegators INT,
  live_saturation DOUBLE PRECISION,
  declared_pledge NUMERIC,
  live_pledge NUMERIC,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS spo_stake_snapshot_updated_at_idx ON spo_stake_snapshot (updated_at DESC);
CREATE INDEX IF NOT EXISTS spo_stake_snapshot_live_stake_idx ON spo_stake_snapshot ((COALESCE(live_stake, 0)) DESC);
--------------------------------------------------------------------------------
-- spo_stake_history
--------------------------------------------------------------------------------
CREATE TABLE spo_stake_history (
  id BIGSERIAL PRIMARY KEY,
  pool_id VARCHAR NOT NULL REFERENCES pool_metadata_cache (pool_id) ON DELETE CASCADE,
  recorded_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  mainchain_epoch INTEGER,
  live_stake NUMERIC,
  active_stake NUMERIC,
  live_delegators INTEGER,
  live_saturation DOUBLE PRECISION,
  declared_pledge NUMERIC,
  live_pledge NUMERIC
);
CREATE INDEX IF NOT EXISTS spo_stake_history_pool_time_idx ON spo_stake_history (pool_id, recorded_at DESC);
CREATE INDEX IF NOT EXISTS spo_stake_history_epoch_idx ON spo_stake_history (mainchain_epoch);
--------------------------------------------------------------------------------
-- spo_stake_refresh_state
--------------------------------------------------------------------------------
CREATE TABLE spo_stake_refresh_state (
  id BOOLEAN PRIMARY KEY DEFAULT TRUE,
  last_pool_id VARCHAR,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
INSERT INTO
  spo_stake_refresh_state (id)
VALUES
  (TRUE)
ON CONFLICT (id) DO NOTHING;
