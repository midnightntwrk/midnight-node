CREATE TYPE CONTRACT_ACTION_VARIANT AS ENUM(
    'Deploy',
    'Call',
    'Update'
);

CREATE TYPE TRANSACTION_VARIANT AS ENUM(
    'Regular',
    'System'
);

CREATE TABLE blocks(
    id BIGSERIAL PRIMARY KEY,
    hash BYTEA NOT NULL UNIQUE,
    height BIGINT NOT NULL UNIQUE,
    protocol_version BIGINT NOT NULL,
    parent_hash BYTEA NOT NULL,
    author BYTEA,
    timestamp BIGINT NOT NULL
);

CREATE TABLE transactions(
    id BIGSERIAL PRIMARY KEY,
    block_id BIGINT NOT NULL REFERENCES blocks(id),
    variant TRANSACTION_VARIANT NOT NULL,
    hash BYTEA NOT NULL,
    protocol_version BIGINT NOT NULL,
    raw BYTEA NOT NULL
);

CREATE INDEX ON transactions(block_id);

CREATE INDEX ON transactions(hash);

CREATE TABLE regular_transactions(
    id BIGINT PRIMARY KEY REFERENCES transactions(id),
    transaction_result JSONB NOT NULL,
    merkle_tree_root BYTEA NOT NULL,
    start_index BIGINT NOT NULL,
    end_index BIGINT NOT NULL,
    paid_fees BYTEA,
    estimated_fees BYTEA,
    identifiers BYTEA[] NOT NULL
);

CREATE INDEX ON regular_transactions(transaction_result);

CREATE INDEX ON regular_transactions USING GIN(transaction_result);

CREATE INDEX ON regular_transactions(start_index);

CREATE INDEX ON regular_transactions(end_index);

CREATE TABLE contract_actions(
    id BIGSERIAL PRIMARY KEY,
    transaction_id BIGINT NOT NULL REFERENCES transactions(id),
    variant CONTRACT_ACTION_VARIANT NOT NULL,
    address BYTEA NOT NULL,
    state BYTEA NOT NULL,
    chain_state BYTEA NOT NULL,
    attributes JSONB NOT NULL
);

CREATE INDEX ON contract_actions(transaction_id);

CREATE INDEX ON contract_actions(address);

CREATE INDEX ON contract_actions(id, address);

CREATE TABLE wallets(
    id UUID PRIMARY KEY,
    session_id BYTEA NOT NULL UNIQUE,
    viewing_key BYTEA NOT NULL, -- Ciphertext with nonce, no longer unique!
    last_indexed_transaction_id BIGINT NOT NULL DEFAULT 0,
    active BOOLEAN NOT NULL DEFAULT TRUE,
    last_active TIMESTAMPTZ NOT NULL
);

CREATE INDEX ON wallets(session_id);

CREATE INDEX ON wallets(last_indexed_transaction_id DESC);

CREATE TABLE relevant_transactions(
    id BIGSERIAL PRIMARY KEY,
    wallet_id UUID NOT NULL REFERENCES wallets(id),
    transaction_id BIGINT NOT NULL REFERENCES transactions(id),
    UNIQUE (wallet_id, transaction_id)
);

CREATE TABLE unshielded_utxos(
    id BIGSERIAL PRIMARY KEY,
    creating_transaction_id BIGINT NOT NULL REFERENCES transactions(id),
    spending_transaction_id BIGINT REFERENCES transactions(id),
    owner BYTEA NOT NULL,
    token_type BYTEA NOT NULL,
    value BYTEA NOT NULL,
    output_index BIGINT NOT NULL,
    intent_hash BYTEA NOT NULL,
    UNIQUE (intent_hash, output_index)
);

CREATE INDEX ON unshielded_utxos(creating_transaction_id);

CREATE INDEX ON unshielded_utxos(spending_transaction_id);

CREATE INDEX ON unshielded_utxos(OWNER);

CREATE INDEX ON unshielded_utxos(token_type);

CREATE TABLE contract_balances(
    id BIGSERIAL PRIMARY KEY,
    contract_action_id BIGINT NOT NULL REFERENCES contract_actions(id),
    token_type BYTEA NOT NULL, -- Serialized TokenType (hex-encoded)
    amount BYTEA NOT NULL, -- u128 amount as bytes (for large number support)
    UNIQUE (contract_action_id, token_type)
);

CREATE INDEX ON contract_balances(contract_action_id);

CREATE INDEX ON contract_balances(token_type);

CREATE INDEX ON contract_balances(contract_action_id, token_type);

