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

D_PERMISSIONED=3
D_REGISTERED=2

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

# sidechain.vkey:aura.vkey:grandpa.vkey
echo "Inserting permissioned candidates for Alice and Bob..."

alice_sidechain_vkey=$(cat /midnight-nodes/midnight-node-1/keys/sidechain.vkey)
alice_aura_vkey=$(cat /midnight-nodes/midnight-node-1/keys/aura.vkey)
alice_grandpa_vkey=$(cat /midnight-nodes/midnight-node-1/keys/grandpa.vkey)

bob_sidechain_vkey=$(cat /midnight-nodes/midnight-node-2/keys/sidechain.vkey)
bob_aura_vkey=$(cat /midnight-nodes/midnight-node-2/keys/aura.vkey)
bob_grandpa_vkey=$(cat /midnight-nodes/midnight-node-2/keys/grandpa.vkey)

cat <<EOF > permissioned_candidates.csv
$alice_sidechain_vkey:$alice_aura_vkey:$alice_grandpa_vkey
$bob_sidechain_vkey:$bob_aura_vkey:$bob_grandpa_vkey
EOF

./midnight-node smart-contracts upsert-permissioned-candidates \
    --genesis-utxo $GENESIS_UTXO \
    --permissioned-candidates-file permissioned_candidates.csv \
    --payment-key-file /keys/funded_address.skey

if [ $? -eq 0 ]; then
    echo "Permissioned candidates Alice and Bob inserted successfully!"
else
    echo "Permission candidates Alice and Bob failed to be added..."
    exit 1
fi

echo "Inserting registered candidate Dave..."

# Prepare Dave registration values
dave_utxo=$(cat /shared/dave.utxo)
dave_mainchain_signing_key=$(jq -r '.cborHex | .[4:]' /midnight-nodes/midnight-node-4/keys/cold.skey)
dave_sidechain_signing_key=$(cat /midnight-nodes/midnight-node-4/keys/sidechain.skey)

# Process registration signatures for Dave
dave_output=$(./midnight-node registration-signatures \
    --genesis-utxo $GENESIS_UTXO \
    --mainchain-signing-key $dave_mainchain_signing_key \
    --sidechain-signing-key $dave_sidechain_signing_key \
    --registration-utxo $dave_utxo)

echo "Dave registration signatures output:"
echo "$dave_output"
# Extract signatures and keys from Dave output
dave_spo_public_key=$(echo "$dave_output" | jq -r ".spo_public_key")
dave_spo_signature=$(echo "$dave_output" | jq -r ".spo_signature")
dave_sidechain_public_key=$(echo "$dave_output" | jq -r ".sidechain_public_key")
dave_sidechain_signature=$(echo "$dave_output" | jq -r ".sidechain_signature")
dave_aura_vkey=$(cat /midnight-nodes/midnight-node-4/keys/aura.vkey)
dave_grandpa_vkey=$(cat /midnight-nodes/midnight-node-4/keys/grandpa.vkey)

# Register Dave
./midnight-node smart-contracts register \
    --genesis-utxo $GENESIS_UTXO \
    --spo-public-key $dave_spo_public_key \
    --spo-signature $dave_spo_signature \
    --sidechain-public-keys $dave_sidechain_public_key:$dave_aura_vkey:$dave_grandpa_vkey \
    --sidechain-signature $dave_sidechain_signature \
    --registration-utxo $dave_utxo \
    --payment-key-file /midnight-nodes/midnight-node-4/keys/payment.skey

if [ $? -eq 0 ]; then
    echo "Registered candidate Dave inserted successfully!"
else
    echo "Registration for Dave failed."
    exit 1
fi

echo "Generating chain-spec.json file for Midnight Nodes..."

cat res/qanet/pc-chain-config.json | jq '.initial_permissioned_candidates |= .[:2]' > /tmp/pc-chain-config-qanet.json

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
export CHAINSPEC_FEDERATED_AUTHORITY_CONFIG=/res/qanet/federated-authority-config.json
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
