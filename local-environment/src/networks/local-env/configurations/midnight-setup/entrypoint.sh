#!/usr/bin/env bash

# This file is part of midnight-node.
# Copyright (C) 2025 Midnight Foundation
# SPDX-License-Identifier: Apache-2.0
# Licensed under the Apache License, Version 2.0 (the "License");
# You may not use this file except in compliance with the License.
# You may obtain a copy of the License at
# http://www.apache.org/licenses/LICENSE-2.0
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

# Fail if a command fails
set -euxo pipefail

apt -qq update
apt -qq -y install curl jq ncat uuid-runtime

check_json_validity() {
  local file="$1"
  if ! jq -e . "$file" > /dev/null 2>&1; then
    echo "Error: $file is invalid JSON."
    exit 1
  fi
}

echo "Using Partner Chains node version:"
./midnight-node --version

set +x # Disable echoing commands

echo "Waiting for Cardano pod to setup genesis..."

while true; do
    if [ -e /shared/genesis.utxo ]; then
        break
    else
        sleep 1
    fi
done

set -x # Re-enable echoing commands

echo "Beginning configuration..."

chmod 644 /shared/shelley/genesis-utxo.skey

echo "Initializing governance authority ..."

export GENESIS_UTXO=$(cat /shared/genesis.utxo)
cat /shared/genesis.utxo
echo "Genesis UTXO: $GENESIS_UTXO"


# export MOCK_REGISTRATIONS_FILE="/node-dev/default-registrations.json"
export POSTGRES_HOST="postgres"
export POSTGRES_PORT="5432"
export POSTGRES_USER="postgres"
if [ ! -f postgres.password ]; then
    uuidgen | tr -d '-' | head -c 16 > postgres.password
fi
export POSTGRES_PASSWORD="$(cat ./postgres.password)"
export POSTGRES_DB="cexplorer"
export DB_SYNC_POSTGRES_CONNECTION_STRING="psql://$POSTGRES_USER:$POSTGRES_PASSWORD@$POSTGRES_HOST:$POSTGRES_PORT/$POSTGRES_DB"
export OGMIOS_URL=http://ogmios:$OGMIOS_PORT

./midnight-node smart-contracts governance init \
    --genesis-utxo $GENESIS_UTXO \
    --payment-key-file /keys/funded_address.skey \
    --governance-authority $GOVERNANCE_AUTHORITY \
    --threshold 1

if [ $? -eq 0 ]; then
   echo "Successfully initialized governance authority!"
else
    echo "Failed to initialize governance authority!"
    exit 1
fi

echo "Generating addresses.json file..."

./midnight-node smart-contracts get-scripts \
    --genesis-utxo $GENESIS_UTXO \
> addresses.json
cat addresses.json

export ADDRESSES_JSON="/addresses.json"
echo "Addresses JSON: $ADDRESSES_JSON"

export COMMITTEE_CANDIDATE_ADDRESS=$(jq -r '.addresses.CommitteeCandidateValidator' addresses.json)
echo "Committee candidate address: $COMMITTEE_CANDIDATE_ADDRESS"

export D_PARAMETER_POLICY_ID=$(jq -r '.policyIds.DParameter' addresses.json)
echo "D parameter policy ID: $D_PARAMETER_POLICY_ID"

export PERMISSIONED_CANDIDATES_POLICY_ID=$(jq -r '.policyIds.PermissionedCandidates' addresses.json)
echo "Permissioned candidates policy ID: $PERMISSIONED_CANDIDATES_POLICY_ID"

echo "Setting values for NATIVE_TOKEN_POLICY_ID, NATIVE_TOKEN_ASSET_NAME, and ILLIQUID_SUPPLY_VALIDATOR_ADDRESS for chain-spec creation"
export NATIVE_TOKEN_POLICY_ID="1fab25f376bc49a181d03a869ee8eaa3157a3a3d242a619ca7995b2b"
export NATIVE_TOKEN_ASSET_NAME="52657761726420746f6b656e"
export ILLIQUID_SUPPLY_VALIDATOR_ADDRESS="addr_test1wpy8ewg646rg4ce78nl3aassmkquf4wlxcaugqlxwzcylkca0q8v3"

echo "Inserting D parameter..."

D_PERMISSIONED=10
D_REGISTERED=0

./midnight-node smart-contracts upsert-d-parameter \
    --genesis-utxo $GENESIS_UTXO \
    --permissioned-candidates-count $D_PERMISSIONED \
    --registered-candidates-count $D_REGISTERED \
    --payment-key-file /keys/funded_address.skey

if [ $? -eq 0 ]; then
    echo "Successfully inserted D-parameter (P = $D_PERMISSIONED, R = $D_REGISTERED)!"
else
    echo "Couldn't insert D-parameter..."
    exit 1
fi

# ============================================================================
# DEPLOY AIKEN GOVERNANCE CONTRACTS
# ============================================================================
echo "Deploying Aiken governance contracts..."

# Wait for contract-compiler to output CBOR files
echo "Waiting for Aiken contract CBOR files..."
RUNTIME_VALUES="/runtime-values"
MAX_WAIT=120
start_time=$(date +%s)
while true; do
    if [[ -f "${RUNTIME_VALUES}/council_forever.cbor" ]] && \
       [[ -f "${RUNTIME_VALUES}/tech_auth_forever.cbor" ]] && \
       [[ -f "${RUNTIME_VALUES}/federated_ops_forever.cbor" ]]; then
        echo "✓ All contract CBOR files found"
        break
    fi
    
    elapsed=$(($(date +%s) - start_time))
    if [[ $elapsed -ge $MAX_WAIT ]]; then
        echo "ERROR: Timeout waiting for contract CBOR files after ${MAX_WAIT}s"
        ls -la "${RUNTIME_VALUES}/" || true
        exit 1
    fi
    
    echo "Waiting for contract CBOR files (${elapsed}s elapsed)..."
    sleep 5
done

# Get the funded address from the shared volume
FUNDED_ADDRESS=$(cat /shared/FUNDED_ADDRESS)
echo "Using funded address: $FUNDED_ADDRESS"

# Read sidechain keys from midnight nodes
alice_sidechain_vkey=$(cat /midnight-nodes/midnight-node-1/keys/sidechain.vkey)
bob_sidechain_vkey=$(cat /midnight-nodes/midnight-node-2/keys/sidechain.vkey)
charlie_sidechain_vkey=$(cat /midnight-nodes/midnight-node-3/keys/sidechain.vkey)
dave_sidechain_vkey=$(cat /midnight-nodes/midnight-node-4/keys/sidechain.vkey)

# Use deterministic Cardano key hashes for testing (28 bytes each)
# These are test values that match the format used in E2E tests
alice_cardano_hash="e8c300330fe315531ca89d4a2e7d0c80211bc70b473b1ed4979dff2a"
bob_cardano_hash="e8c300330fe315531ca89d4a2e7d0c80211bc70b473b1ed4979dff2b"
charlie_cardano_hash="e8c300330fe315531ca89d4a2e7d0c80211bc70b473b1ed4979dff2c"
dave_cardano_hash="e8c300330fe315531ca89d4a2e7d0c80211bc70b473b1ed4979dff2d"

# Create members.json for council_forever contract
cat <<EOF > council_members.json
[
  {"cardano_hash": "$alice_cardano_hash", "sr25519_key": "$alice_sidechain_vkey"},
  {"cardano_hash": "$bob_cardano_hash", "sr25519_key": "$bob_sidechain_vkey"},
  {"cardano_hash": "$charlie_cardano_hash", "sr25519_key": "$charlie_sidechain_vkey"},
  {"cardano_hash": "$dave_cardano_hash", "sr25519_key": "$dave_sidechain_vkey"}
]
EOF

echo "Created council_members.json:"
cat council_members.json

# Read one-shot UTxO references
COUNCIL_ONESHOT_HASH=$(cat ${RUNTIME_VALUES}/council_oneshot_hash.txt | tr -d '\n\r')
COUNCIL_ONESHOT_INDEX=$(cat ${RUNTIME_VALUES}/council_oneshot_index.txt | tr -d '\n\r')
TECHAUTH_ONESHOT_HASH=$(cat ${RUNTIME_VALUES}/techauth_oneshot_hash.txt | tr -d '\n\r')
TECHAUTH_ONESHOT_INDEX=$(cat ${RUNTIME_VALUES}/techauth_oneshot_index.txt | tr -d '\n\r')
FEDOPS_ONESHOT_HASH=$(cat ${RUNTIME_VALUES}/federatedops_oneshot_hash.txt | tr -d '\n\r')
FEDOPS_ONESHOT_INDEX=$(cat ${RUNTIME_VALUES}/federatedops_oneshot_index.txt | tr -d '\n\r')

echo "One-shot UTxO references:"
echo "  Council: ${COUNCIL_ONESHOT_HASH}#${COUNCIL_ONESHOT_INDEX}"
echo "  Tech Auth: ${TECHAUTH_ONESHOT_HASH}#${TECHAUTH_ONESHOT_INDEX}"
echo "  Federated Ops: ${FEDOPS_ONESHOT_HASH}#${FEDOPS_ONESHOT_INDEX}"

# Get signing key CBOR (extract cborHex from skey file, skip first 4 chars)
SIGNING_KEY_CBOR=$(jq -r '.cborHex | .[4:]' /keys/funded_address.skey)
echo "$SIGNING_KEY_CBOR" > /tmp/signing_key.cbor

# Skip governance contract deployment if SKIP_GOVERNANCE_DEPLOY is set
# This is used when E2E tests need to deploy the contracts themselves with specific test data
if [ "${SKIP_GOVERNANCE_DEPLOY:-false}" = "true" ]; then
    echo ""
    echo "=== Skipping Governance Contract Deployment (SKIP_GOVERNANCE_DEPLOY=true) ==="
    echo "One-shot UTxOs preserved for E2E test deployment:"
    echo "  Council: ${COUNCIL_ONESHOT_HASH}#${COUNCIL_ONESHOT_INDEX}"
    echo "  Tech Auth: ${TECHAUTH_ONESHOT_HASH}#${TECHAUTH_ONESHOT_INDEX}"
    echo "  Federated Ops: ${FEDOPS_ONESHOT_HASH}#${FEDOPS_ONESHOT_INDEX}"
else
    # Deploy council_forever contract
    echo ""
    echo "=== Deploying Council Forever Contract ==="
    ./aiken-deployer \
        --contract-cbor "${RUNTIME_VALUES}/council_forever.cbor" \
        --one-shot-utxo "${COUNCIL_ONESHOT_HASH}#${COUNCIL_ONESHOT_INDEX}" \
        --signing-key /tmp/signing_key.cbor \
        --funded-address "$FUNDED_ADDRESS" \
        --members-file council_members.json \
        --ogmios-url "$OGMIOS_URL"

    if [ $? -eq 0 ]; then
        echo "✓ Council Forever contract deployed successfully!"
    else
        echo "✗ Council Forever contract deployment failed"
        exit 1
    fi

    # Wait for transaction to confirm
    sleep 10

    # Deploy tech_auth_forever contract (uses same members for testing)
    echo ""
    echo "=== Deploying Tech Auth Forever Contract ==="
    ./aiken-deployer \
        --contract-cbor "${RUNTIME_VALUES}/tech_auth_forever.cbor" \
        --one-shot-utxo "${TECHAUTH_ONESHOT_HASH}#${TECHAUTH_ONESHOT_INDEX}" \
        --signing-key /tmp/signing_key.cbor \
        --funded-address "$FUNDED_ADDRESS" \
        --members-file council_members.json \
        --ogmios-url "$OGMIOS_URL" \
        --contract-type tech-auth

    if [ $? -eq 0 ]; then
        echo "✓ Tech Auth Forever contract deployed successfully!"
    else
        echo "✗ Tech Auth Forever contract deployment failed"
        exit 1
    fi

    # Wait for transaction to confirm
    sleep 10

    # Deploy federated_ops_forever contract
    # Note: federated-ops uses a different datum structure (FederatedOps with appendix field)
    echo ""
    echo "=== Deploying Federated Ops Forever Contract ==="
    ./aiken-deployer \
        --contract-cbor "${RUNTIME_VALUES}/federated_ops_forever.cbor" \
        --one-shot-utxo "${FEDOPS_ONESHOT_HASH}#${FEDOPS_ONESHOT_INDEX}" \
        --signing-key /tmp/signing_key.cbor \
        --funded-address "$FUNDED_ADDRESS" \
        --members-file council_members.json \
        --ogmios-url "$OGMIOS_URL" \
        --contract-type federated-ops

    if [ $? -eq 0 ]; then
        echo "✓ Federated Ops Forever contract deployed successfully!"
    else
        echo "✗ Federated Ops Forever contract deployment failed"
        exit 1
    fi

    echo ""
    echo "=== All Aiken Governance Contracts Deployed Successfully ==="
fi

echo "Inserting registered candidate Eve..."

# Prepare Eve registration values
eve_utxo=$(cat /shared/eve.utxo)
eve_mainchain_signing_key=$(jq -r '.cborHex | .[4:]' /midnight-nodes/midnight-node-5/keys/cold.skey)
eve_sidechain_signing_key=$(cat /midnight-nodes/midnight-node-5/keys/sidechain.skey)

# Process registration signatures for Eve
eve_output=$(./midnight-node registration-signatures \
    --genesis-utxo $GENESIS_UTXO \
    --mainchain-signing-key $eve_mainchain_signing_key \
    --sidechain-signing-key $eve_sidechain_signing_key \
    --registration-utxo $eve_utxo)

echo "Eve registration signatures output:"
echo "$eve_output"
# Extract signatures and keys from Eve output
eve_spo_public_key=$(echo "$eve_output" | jq -r ".spo_public_key")
eve_spo_signature=$(echo "$eve_output" | jq -r ".spo_signature")
eve_sidechain_public_key=$(echo "$eve_output" | jq -r ".sidechain_public_key")
eve_sidechain_signature=$(echo "$eve_output" | jq -r ".sidechain_signature")
eve_aura_vkey=$(cat /midnight-nodes/midnight-node-5/keys/aura.vkey)
eve_grandpa_vkey=$(cat /midnight-nodes/midnight-node-5/keys/grandpa.vkey)

# Register Eve
./midnight-node smart-contracts register \
    --genesis-utxo $GENESIS_UTXO \
    --spo-public-key $eve_spo_public_key \
    --spo-signature $eve_spo_signature \
    --sidechain-public-keys $eve_sidechain_public_key:$eve_aura_vkey:$eve_grandpa_vkey \
    --sidechain-signature $eve_sidechain_signature \
    --registration-utxo $eve_utxo \
    --payment-key-file /midnight-nodes/midnight-node-5/keys/payment.skey

if [ $? -eq 0 ]; then
    echo "Registered candidate Eve inserted successfully!"
else
    echo "Registration for Eve failed."
    exit 1
fi

echo "Generating chain-spec.json file for Midnight Nodes..."

cat res/qanet/pc-chain-config.json | jq '.initial_permissioned_candidates |= .[:4]' > /tmp/pc-chain-config-qanet.json

jq 'env as $env | . + {
  "chain_parameters": {
    "genesis_utxo": $env.GENESIS_UTXO
  },
  "cardano_addresses": {
    "committee_candidates_address": $env.COMMITTEE_CANDIDATE_ADDRESS,
    "d_parameter_policy_id": $env.D_PARAMETER_POLICY_ID,
    "permissioned_candidates_policy_id": $env.PERMISSIONED_CANDIDATES_POLICY_ID,
  }
}' /tmp/pc-chain-config-qanet.json > /tmp/pc-chain-config.json

export CHAINSPEC_NAME=localenv1
export CHAINSPEC_ID=localenv
export CHAINSPEC_NETWORK_ID=devnet
export CHAINSPEC_GENESIS_STATE=res/genesis/genesis_state_undeployed.mn
export CHAINSPEC_GENESIS_BLOCK=res/genesis/genesis_block_undeployed.mn
export CHAINSPEC_GENESIS_TX=res/genesis/genesis_tx_undeployed.mn  #  0.13.5 compatibility, can be removed in the future
export CHAINSPEC_CHAIN_TYPE=live
export CHAINSPEC_PC_CHAIN_CONFIG=/tmp/pc-chain-config.json
export CHAINSPEC_CNIGHT_GENESIS=res/qanet/cnight-genesis.json
export CHAINSPEC_FEDERATED_AUTHORITY_CONFIG=/res/dev/federated-authority-config.json
export CHAINSPEC_SYSTEM_PARAMETERS_CONFIG=/res/dev/system-parameters-config.json
./midnight-node build-spec --disable-default-bootnode > chain-spec.json
echo "chain-spec.json file generated."

echo "Amending the chain spec..."
echo "Configuring Epoch Length..."
jq '.genesis.runtimeGenesis.config.sidechain.slotsPerEpoch = 5' chain-spec.json > tmp.json && mv tmp.json chain-spec.json

check_json_validity chain-spec.json

echo "Final chain spec"

echo "Copying chain-spec.json file to /shared/chain-spec.json..."
cp chain-spec.json /shared/chain-spec.json
echo "chain-spec.json generation complete."

echo "Partnerchain configuration is complete, and will be able to start after two mainchain epochs."

echo -e "\n===== Partnerchain Configuration Complete =====\n"
echo -e "Container will now idle, but will remain available for accessing the midnight-node commands as follows:\n"
echo "docker exec midnight-setup midnight-node smart-contracts --help"

echo "Waiting 2 epochs for DParam to become active..."
epoch=$(curl -s --request POST \
    --url "http://ogmios:1337" \
    --header 'Content-Type: application/json' \
    --data '{"jsonrpc": "2.0", "method": "queryLedgerState/epoch"}' | jq .result)
n_2_epoch=$((epoch + 2))
echo "Current epoch: $epoch"
while [ $epoch -lt $n_2_epoch ]; do
  sleep 10
  epoch=$(curl -s --request POST \
    --url "http://ogmios:1337" \
    --header 'Content-Type: application/json' \
    --data '{"jsonrpc": "2.0", "method": "queryLedgerState/epoch"}' | jq .result)
  echo "Current epoch: $epoch"
done
echo "DParam is now active!"
