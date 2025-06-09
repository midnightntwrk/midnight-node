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

#!/bin/bash

apt -qq update &> /dev/null
apt -qq -y install curl jq ncat &> /dev/null

check_json_validity() {
  local file="$1"
  if ! jq -e . "$file" > /dev/null 2>&1; then
    echo "Error: $file is invalid JSON."
    exit 1
  fi
}

cp /usr/local/bin/midnight-node /midnight-node
echo "Using Partner Chains node version:"
./midnight-node --version

echo "Waiting for the Cardano network to sync and for Ogmios to start..."

while true; do
    if nc -z ogmios $OGMIOS_PORT; then
        break
    else
        sleep 1
    fi
done

echo "Beginning configuration..."

chmod 644 /shared/shelley/genesis-utxo.skey

echo "Initializing governance authority ..."

export GENESIS_UTXO=$(cat /shared/genesis.utxo)
cat /shared/genesis.utxo
echo "Genesis UTXO: $GENESIS_UTXO"


export MAIN_CHAIN_FOLLOWER_MOCK_REGISTRATIONS_FILE="/node-dev/default-registrations.json"
export POSTGRES_HOST="postgres"
export POSTGRES_PORT="5432"
export POSTGRES_USER="postgres"
if [ ! -f postgres.password ]; then
    uuidgen | tr -d '-' | head -c 16 > postgres.password
fi
export POSTGRES_PASSWORD="$(cat ./postgres.password)"
export POSTGRES_DB="cexplorer"
export DB_SYNC_POSTGRES_CONNECTION_STRING="psql://$POSTGRES_USER:$POSTGRES_PASSWORD@$POSTGRES_HOST:$POSTGRES_PORT/$POSTGRES_DB"

./midnight-node smart-contracts governance init \
    --ogmios-url http://ogmios:$OGMIOS_PORT \
    --genesis-utxo $GENESIS_UTXO \
    --payment-key-file /keys/funded_address.skey \
    --governance-authority $GOVERNANCE_AUTHORITY 
    # --threshold 1

if [ $? -eq 0 ]; then
   echo "Successfully initialized governance authority!"
else
    echo "Failed to initialize governance authority!"
    exit 1
fi

echo "Generating addresses.json file..."

./midnight-node smart-contracts get-scripts \
    --ogmios-url http://ogmios:$OGMIOS_PORT \
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

./midnight-node smart-contracts upsert-d-parameter \
    --ogmios-url http://ogmios:$OGMIOS_PORT \
    --genesis-utxo $GENESIS_UTXO \
    --permissioned-candidates-count 3 \
    --registered-candidates-count 2 \
    --payment-key-file /keys/funded_address.skey

if [ $? -eq 0 ]; then
    echo "Successfully inserted D-parameter (P = 3, R = 2)!"
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
    --ogmios-url http://ogmios:$OGMIOS_PORT \
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
    --ogmios-url http://ogmios:$OGMIOS_PORT \
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

echo "Generating chain-spec.json file for Partnerchain Nodes..."
./midnight-node build-spec --disable-default-bootnode > chain-spec.json
echo "chain-spec.json file generated."
cat chain-spec.json
echo "Configuring chain-spec.json file..."

echo "Amending the chain spec..."


echo "Configuring Initial Validators..."
jq '.genesis.runtimeGenesis.config.session.initialValidators = [
     [
         "5C7C2Z5sWbytvHpuLTvzKunnnRwQxft1jiqrLD5rhucQ5S9X",
         {
             "aura": "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY",
             "grandpa": "5FA9nQDVg267DEd8m1ZypXLBnvN7SFxYwV7ndqSYGiN9TTpu"
         }
     ],
     [
         "5DVskgSC9ncWQpxFMeUn45NU43RUq93ByEge6ApbnLk6BR9N",
         {
             "aura": "5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty",
             "grandpa": "5GoNkf6WdbxCFnPdAnYYQyCjAKPJgLNxXwPjwTh6DGg6gN3E"
         }
     ],
     [
         "5EP2cMaCxLzhfD3aFAqqgu3kfXH7GcwweEv6JXZRP6ysRHkQ",
         {
             "aura": "5FLSigC9HGRKVhB9FiEo4Y3koPsNmBmLJbpXg2mp1hXcS59Y",
             "grandpa": "5DbKjhNLpqX3zqZdNBc9BGb4fHU1cRBaDhJUskrvkwfraDi6"
         }
     ]
 ]' chain-spec.json > tmp.json && mv tmp.json chain-spec.json
echo "Amending main chain scripts..."

check_json_validity chain-spec.json

echo "Configuring Initial Authorities..."
jq '.genesis.runtimeGenesis.config.sessionCommitteeManagement.initialAuthorities = [
  [
    "KW39r9CJjAVzmkf9zQ4YDb2hqfAVGdRqn53eRqyruqpxAP5YL",
    {
      "aura": "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY",
      "grandpa": "5FA9nQDVg267DEd8m1ZypXLBnvN7SFxYwV7ndqSYGiN9TTpu"
    }
  ],
  [
    "KWByAN7WfZABWS5AoWqxriRmF5f2jnDqy3rB5pfHLGkY93ibN",
    {
      "aura": "5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty",
      "grandpa": "5GoNkf6WdbxCFnPdAnYYQyCjAKPJgLNxXwPjwTh6DGg6gN3E"
    }
  ],
  [
    "KWBpGtyJLBkJERdZT1a1uu19c2uPpZm9nFd8SGtCfRUAT3Y4w",
    {
      "aura": "5FLSigC9HGRKVhB9FiEo4Y3koPsNmBmLJbpXg2mp1hXcS59Y",
      "grandpa": "5DbKjhNLpqX3zqZdNBc9BGb4fHU1cRBaDhJUskrvkwfraDi6"
    }
  ]
]' chain-spec.json > tmp.json && mv tmp.json chain-spec.json

check_json_validity chain-spec.json

echo "Set initial funds to Alice (ecdsa), ?, and Alice (sr25519)"
jq '.genesis.runtimeGenesis.config.balances.balances = [
    ["5C7C2Z5sWbytvHpuLTvzKunnnRwQxft1jiqrLD5rhucQ5S9X", 1000000000000000],
    ["5D9eDKbFt4JKaEndQvMmbJYnpX9ENUj8U9UUg1AxSa64FJxE", 1000000000000000],
    ["5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY", 1000000000000000]
]' chain-spec.json > tmp.json && mv tmp.json chain-spec.json

check_json_validity chain-spec.json

echo "Configuring Alice as sudo..."
jq '.genesis.runtimeGenesis.config.sudo = {
    "key": "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY"
}' chain-spec.json > tmp.json && mv tmp.json chain-spec.json

echo "Configuring Epoch Length..."
jq '.genesis.runtimeGenesis.config.sidechain.slotsPerEpoch = 5' chain-spec.json > tmp.json && mv tmp.json chain-spec.json
echo "Final chain spec"
cat chain-spec.json

echo "Copying chain-spec.json file to /shared/chain-spec.json..."
cp chain-spec.json /shared/chain-spec.json
echo "chain-spec.json generation complete."

echo "Partnerchain configuration is complete, and will be able to start after two mainchain epochs."
touch /shared/midnight-setup.ready

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

tail -f /dev/null
