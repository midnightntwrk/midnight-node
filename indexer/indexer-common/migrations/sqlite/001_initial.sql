--------------------------------------------------------------------------------
-- blocks
--------------------------------------------------------------------------------
CREATE TABLE blocks (
  id INTEGER PRIMARY KEY,
  hash BLOB NOT NULL UNIQUE,
  height INTEGER NOT NULL,
  protocol_version INTEGER NOT NULL,
  parent_hash BLOB NOT NULL,
  author BLOB,
  timestamp INTEGER NOT NULL,
  zswap_merkle_tree_root BLOB NOT NULL,
  ledger_parameters BLOB NOT NULL,
  ledger_state_key BLOB NOT NULL
);
--------------------------------------------------------------------------------
-- transactions
--------------------------------------------------------------------------------
CREATE TABLE transactions (
  id INTEGER PRIMARY KEY,
  block_id INTEGER NOT NULL REFERENCES blocks (id),
  variant TEXT CHECK (variant IN ('Regular', 'System')) NOT NULL,
  hash BLOB NOT NULL,
  protocol_version INTEGER NOT NULL,
  raw BLOB NOT NULL
);
CREATE INDEX transactions_block_id_idx ON transactions (block_id);
CREATE INDEX transactions_hash_idx ON transactions (hash);
CREATE INDEX transactions_variant_id_idx ON transactions (variant, id);
--------------------------------------------------------------------------------
-- regular_transactions
--------------------------------------------------------------------------------
CREATE TABLE regular_transactions (
  id INTEGER PRIMARY KEY REFERENCES transactions (id),
  transaction_result TEXT NOT NULL,
  zswap_merkle_tree_root BLOB NOT NULL,
  zswap_start_index INTEGER NOT NULL,
  zswap_end_index INTEGER NOT NULL,
  dust_commitment_start_index INTEGER NOT NULL,
  dust_commitment_end_index INTEGER NOT NULL,
  dust_generation_start_index INTEGER NOT NULL,
  dust_generation_end_index INTEGER NOT NULL,
  paid_fees BLOB,
  estimated_fees BLOB
);
CREATE INDEX regular_transactions_transaction_result_idx ON regular_transactions (transaction_result);
CREATE INDEX regular_transactions_zswap_start_idx ON regular_transactions (zswap_start_index);
CREATE INDEX regular_transactions_zswap_end_idx ON regular_transactions (zswap_end_index);
--------------------------------------------------------------------------------
-- transaction_identifiers
--------------------------------------------------------------------------------
CREATE TABLE transaction_identifiers (
  id INTEGER PRIMARY KEY,
  transaction_id INTEGER NOT NULL REFERENCES regular_transactions (id),
  identifier BLOB NOT NULL
);
CREATE INDEX transaction_identifiers_transaction_id_idx ON transaction_identifiers (transaction_id);
CREATE INDEX transaction_identifiers_identifier_idx ON transaction_identifiers (identifier);
--------------------------------------------------------------------------------
-- contract_actions
--------------------------------------------------------------------------------
CREATE TABLE contract_actions (
  id INTEGER PRIMARY KEY,
  transaction_id INTEGER NOT NULL REFERENCES transactions (id),
  variant TEXT CHECK (variant IN ('Deploy', 'Call', 'Update')) NOT NULL,
  address BLOB NOT NULL,
  state BLOB NOT NULL,
  zswap_state BLOB NOT NULL,
  attributes TEXT NOT NULL
);
CREATE INDEX contract_actions_transaction_id_idx ON contract_actions (transaction_id);
CREATE INDEX contract_actions_address_idx ON contract_actions (address);
CREATE INDEX contract_actions_id_address_idx ON contract_actions (id, address);
--------------------------------------------------------------------------------
-- unshielded_utxos
--------------------------------------------------------------------------------
CREATE TABLE unshielded_utxos (
  id INTEGER PRIMARY KEY,
  creating_transaction_id INTEGER NOT NULL REFERENCES transactions (id),
  spending_transaction_id INTEGER REFERENCES transactions (id),
  owner BLOB NOT NULL,
  token_type BLOB NOT NULL,
  value BLOB NOT NULL,
  intent_hash BLOB NOT NULL,
  output_index INTEGER NOT NULL,
  ctime INTEGER,
  initial_nonce BLOB NOT NULL,
  registered_for_dust_generation INTEGER NOT NULL,
  UNIQUE (intent_hash, output_index)
);
CREATE INDEX unshielded_creating_idx ON unshielded_utxos (creating_transaction_id);
CREATE INDEX unshielded_spending_idx ON unshielded_utxos (spending_transaction_id);
CREATE INDEX unshielded_owner_idx ON unshielded_utxos (owner);
CREATE INDEX unshielded_token_type_idx ON unshielded_utxos (token_type);
--------------------------------------------------------------------------------
-- ledger_events
--------------------------------------------------------------------------------
CREATE TABLE ledger_events (
  id INTEGER PRIMARY KEY,
  transaction_id INTEGER NOT NULL REFERENCES transactions (id),
  variant TEXT CHECK (
    variant IN (
      'ZswapInput',
      'ZswapOutput',
      'ParamChange',
      'DustInitialUtxo',
      'DustGenerationDtimeUpdate',
      'DustSpendProcessed'
    )
  ) NOT NULL,
  grouping TEXT CHECK (grouping IN ('Zswap', 'Dust')) NOT NULL,
  raw BYTEA NOT NULL,
  attributes TEXT NOT NULL
);
CREATE INDEX ledger_events_transaction_id_idx ON ledger_events (transaction_id);
CREATE INDEX ledger_events_variant_idx ON ledger_events (variant);
CREATE INDEX ledger_events_grouping_idx ON ledger_events (grouping);
CREATE INDEX ledger_events_id_grouping_idx ON ledger_events (id, grouping);
CREATE INDEX ledger_events_transaction_id_grouping_idx ON ledger_events (transaction_id, grouping);
--------------------------------------------------------------------------------
-- contract_balances
--------------------------------------------------------------------------------
CREATE TABLE contract_balances (
  id INTEGER PRIMARY KEY,
  contract_action_id INTEGER NOT NULL REFERENCES contract_actions (id),
  token_type BLOB NOT NULL, -- Serialized TokenType (hex-encoded)
  amount BLOB NOT NULL, -- u128 amount as bytes (for large number support)
  UNIQUE (contract_action_id, token_type)
);
CREATE INDEX contract_balances_action_idx ON contract_balances (contract_action_id);
CREATE INDEX contract_balances_token_type_idx ON contract_balances (token_type);
CREATE INDEX contract_balances_action_token_idx ON contract_balances (contract_action_id, token_type);
--------------------------------------------------------------------------------
-- wallets
--------------------------------------------------------------------------------
CREATE TABLE wallets (
  id BLOB PRIMARY KEY, -- UUID
  viewing_key_hash BLOB NOT NULL UNIQUE,
  viewing_key BLOB NOT NULL, -- Ciphertext with nonce, no longer unique!
  wanted_start_index INTEGER NOT NULL DEFAULT 0,
  first_indexed_transaction_id INTEGER NOT NULL DEFAULT 0,
  last_indexed_transaction_id INTEGER NOT NULL DEFAULT 0,
  last_active INTEGER NOT NULL,
  session_id BLOB UNIQUE -- Random per-session ID for API authentication, NULL when disconnected.
);
CREATE INDEX wallets_viewing_key_hash_idx ON wallets (viewing_key_hash);
CREATE INDEX wallets_last_indexed_transaction_id_idx ON wallets (last_indexed_transaction_id DESC);
--------------------------------------------------------------------------------
-- relevant_transactions
--------------------------------------------------------------------------------
CREATE TABLE relevant_transactions (
  id INTEGER PRIMARY KEY,
  wallet_id BLOB NOT NULL REFERENCES wallets (id),
  transaction_id INTEGER NOT NULL REFERENCES transactions (id),
  UNIQUE (wallet_id, transaction_id)
);
--------------------------------------------------------------------------------
-- DUST Generation Status Tables
-- These tables support the dustGenerationStatus GraphQL query for Protofire dApp
--------------------------------------------------------------------------------
-- DUST generation information tracking
CREATE TABLE dust_generation_info (
  id INTEGER PRIMARY KEY,
  night_utxo_hash BLOB NOT NULL,
  value BLOB NOT NULL,
  owner BLOB NOT NULL,
  nonce BLOB NOT NULL,
  ctime INTEGER NOT NULL,
  merkle_index INTEGER NOT NULL,
  dtime INTEGER,
  transaction_id INTEGER REFERENCES transactions (id)
);
CREATE INDEX dust_generation_info_owner_idx ON dust_generation_info (owner);
CREATE INDEX dust_generation_info_night_utxo_hash_idx ON dust_generation_info (night_utxo_hash);
CREATE INDEX dust_generation_info_transaction_id_idx ON dust_generation_info (transaction_id);
CREATE TABLE dust_nullifiers (
  id INTEGER PRIMARY KEY,
  nullifier BLOB NOT NULL,
  commitment BLOB NOT NULL,
  transaction_id INTEGER NOT NULL REFERENCES transactions (id),
  block_id INTEGER NOT NULL REFERENCES blocks (id)
);
CREATE INDEX dust_nullifiers_nullifier_idx ON dust_nullifiers (nullifier);
CREATE INDEX dust_nullifiers_transaction_id_idx ON dust_nullifiers (transaction_id);
CREATE INDEX dust_nullifiers_block_id_idx ON dust_nullifiers (block_id);
-- Zswap (shielded) nullifier tracking for shieldedNullifierTransactions subscription
CREATE TABLE zswap_nullifiers (
  id INTEGER PRIMARY KEY,
  transaction_id INTEGER NOT NULL REFERENCES transactions (id),
  block_id INTEGER NOT NULL REFERENCES blocks (id),
  nullifier BLOB NOT NULL
);
CREATE INDEX zswap_nullifiers_nullifier_idx ON zswap_nullifiers (nullifier);
CREATE INDEX zswap_nullifiers_transaction_id_idx ON zswap_nullifiers (transaction_id);
CREATE INDEX zswap_nullifiers_block_id_idx ON zswap_nullifiers (block_id);
-- cNIGHT registration tracking
CREATE TABLE cnight_registrations (
  id INTEGER PRIMARY KEY,
  cardano_stake_key BLOB NOT NULL,
  dust_address BLOB NOT NULL,
  valid BOOLEAN NOT NULL,
  registered_at INTEGER NOT NULL,
  removed_at INTEGER,
  block_id INTEGER REFERENCES blocks (id),
  utxo_tx_hash BLOB,
  utxo_output_index INTEGER,
  UNIQUE (cardano_stake_key, dust_address)
);
CREATE INDEX cnight_registrations_cardano_stake_key_idx ON cnight_registrations (cardano_stake_key);
CREATE INDEX cnight_registrations_dust_address_idx ON cnight_registrations (dust_address);
CREATE INDEX cnight_registrations_block_id_idx ON cnight_registrations (block_id);
CREATE TABLE system_parameters_terms_and_conditions (
  id INTEGER PRIMARY KEY,
  block_height INTEGER NOT NULL,
  block_hash BLOB NOT NULL,
  timestamp INTEGER NOT NULL,
  hash BLOB NOT NULL,
  url TEXT NOT NULL
);
CREATE INDEX system_parameters_tc_block_height_idx ON system_parameters_terms_and_conditions (block_height DESC);
CREATE TABLE system_parameters_d (
  id INTEGER PRIMARY KEY,
  block_height INTEGER NOT NULL,
  block_hash BLOB NOT NULL,
  timestamp INTEGER NOT NULL,
  num_permissioned_candidates INTEGER NOT NULL,
  num_registered_candidates INTEGER NOT NULL
);
CREATE INDEX system_parameters_d_block_height_idx ON system_parameters_d (block_height DESC);
--------------------------------------------------------------------------------
-- epochs
--------------------------------------------------------------------------------
CREATE TABLE epochs (
  epoch_no INTEGER PRIMARY KEY,
  starts_at TEXT NOT NULL,
  ends_at TEXT NOT NULL
);
--------------------------------------------------------------------------------
-- pool_metadata_cache
--------------------------------------------------------------------------------
CREATE TABLE pool_metadata_cache (
  pool_id TEXT PRIMARY KEY,
  hex_id TEXT UNIQUE,
  name TEXT,
  ticker TEXT,
  homepage_url TEXT,
  updated_at TEXT,
  url TEXT
);
--------------------------------------------------------------------------------
-- spo_identity
--------------------------------------------------------------------------------
CREATE TABLE spo_identity (
  spo_sk TEXT PRIMARY KEY,
  sidechain_pubkey TEXT UNIQUE,
  pool_id TEXT REFERENCES pool_metadata_cache (pool_id),
  mainchain_pubkey TEXT UNIQUE,
  aura_pubkey TEXT UNIQUE
);
CREATE INDEX spo_identity_pk ON spo_identity (pool_id, sidechain_pubkey, aura_pubkey);
--------------------------------------------------------------------------------
-- committee_membership
--------------------------------------------------------------------------------
CREATE TABLE committee_membership (
  spo_sk TEXT,
  sidechain_pubkey TEXT,
  epoch_no INTEGER NOT NULL,
  position INTEGER NOT NULL,
  expected_slots INTEGER NOT NULL,
  PRIMARY KEY (epoch_no, position)
);
CREATE INDEX committee_membership_epoch_no_idx ON committee_membership (epoch_no);
--------------------------------------------------------------------------------
-- spo_epoch_performance
--------------------------------------------------------------------------------
CREATE TABLE spo_epoch_performance (
  spo_sk TEXT REFERENCES spo_identity (spo_sk),
  identity_label TEXT,
  epoch_no INTEGER NOT NULL,
  expected_blocks INTEGER NOT NULL,
  produced_blocks INTEGER NOT NULL,
  PRIMARY KEY (epoch_no, spo_sk)
);
CREATE INDEX spo_epoch_performance_identity_pk ON spo_epoch_performance (epoch_no, identity_label);
CREATE INDEX spo_epoch_performance_epoch_no_idx ON spo_epoch_performance (epoch_no);
--------------------------------------------------------------------------------
-- spo_history
--------------------------------------------------------------------------------
CREATE TABLE spo_history (
  spo_hist_sk INTEGER PRIMARY KEY,
  spo_sk TEXT REFERENCES spo_identity (spo_sk),
  epoch_no INTEGER NOT NULL,
  status TEXT NOT NULL,
  valid_from INTEGER NOT NULL,
  valid_to INTEGER NOT NULL,
  UNIQUE (spo_sk, epoch_no)
);
CREATE INDEX spo_history_epoch_no_idx ON spo_history (epoch_no);
--------------------------------------------------------------------------------
-- spo_stake_snapshot
--------------------------------------------------------------------------------
CREATE TABLE spo_stake_snapshot (
  pool_id TEXT PRIMARY KEY REFERENCES pool_metadata_cache (pool_id) ON DELETE CASCADE,
  live_stake REAL,
  active_stake REAL,
  live_delegators INTEGER,
  live_saturation REAL,
  declared_pledge REAL,
  live_pledge REAL,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX spo_stake_snapshot_updated_at_idx ON spo_stake_snapshot (updated_at DESC);
CREATE INDEX spo_stake_snapshot_live_stake_idx ON spo_stake_snapshot (COALESCE(live_stake, 0) DESC);
--------------------------------------------------------------------------------
-- spo_stake_history
--------------------------------------------------------------------------------
CREATE TABLE spo_stake_history (
  id INTEGER PRIMARY KEY,
  pool_id TEXT NOT NULL REFERENCES pool_metadata_cache (pool_id) ON DELETE CASCADE,
  recorded_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  mainchain_epoch INTEGER,
  live_stake REAL,
  active_stake REAL,
  live_delegators INTEGER,
  live_saturation REAL,
  declared_pledge REAL,
  live_pledge REAL
);
CREATE INDEX spo_stake_history_pool_time_idx ON spo_stake_history (pool_id, recorded_at DESC);
CREATE INDEX spo_stake_history_epoch_idx ON spo_stake_history (mainchain_epoch);
--------------------------------------------------------------------------------
-- spo_stake_refresh_state
--------------------------------------------------------------------------------
CREATE TABLE spo_stake_refresh_state (
  id INTEGER PRIMARY KEY DEFAULT 1,
  last_pool_id TEXT,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
INSERT INTO
  spo_stake_refresh_state (id)
VALUES
  (1)
ON CONFLICT (id) DO NOTHING;
