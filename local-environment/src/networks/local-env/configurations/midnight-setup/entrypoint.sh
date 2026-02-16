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

microdnf -y update
microdnf -y install curl-minimal jq nmap-ncat util-linux

check_json_validity() {
  local file="$1"
  if ! jq -e . "$file" > /dev/null 2>&1; then
    echo "Error: $file is invalid JSON."
    exit 1
  fi
}

# Contracts deployed, get current epoch to know when it will be active
epoch=$(curl -s --request POST \
    --url "http://ogmios:1337" \
    --header 'Content-Type: application/json' \
    --data '{"jsonrpc": "2.0", "method": "queryLedgerState/epoch"}' | jq .result)

echo "Using Partner Chains node version:"
./midnight-node --version

export POSTGRES_HOST="postgres"
export POSTGRES_PORT="5432"
export POSTGRES_USER="postgres"
if [ ! -f postgres.password ]; then
    uuidgen | tr -d '-' | head -c 16 > postgres.password
fi
POSTGRES_PASSWORD="$(cat ./postgres.password)"
export POSTGRES_PASSWORD
export POSTGRES_DB="cexplorer"
export DB_SYNC_POSTGRES_CONNECTION_STRING="psql://$POSTGRES_USER:$POSTGRES_PASSWORD@$POSTGRES_HOST:$POSTGRES_PORT/$POSTGRES_DB"
export OGMIOS_URL=http://ogmios:$OGMIOS_PORT

D_PERMISSIONED=3
D_REGISTERED=0
CONTRACT_INFO="/runtime-values/contracts-info.json"
COUNCIL_POLICY_ID=$(jq -r '.[] | select(.name == "Council Forever") | .scriptHash' $CONTRACT_INFO)
COUNCIL_SCRIPT_ADDRESS=$(jq -r '.[] | select(.name == "Council Forever") | .address' $CONTRACT_INFO)
TECHAUTH_POLICY_ID=$(jq -r '.[] | select(.name == "Tech Auth Forever") | .scriptHash' $CONTRACT_INFO)
TECHAUTH_SCRIPT_ADDRESS=$(jq -r '.[] | select(.name == "Tech Auth Forever") | .address' $CONTRACT_INFO)
export PERMISSIONED_CANDIDATES_POLICY_ID=$(jq -r '.[] | select(.name == "Federated Ops Forever") | .scriptHash' $CONTRACT_INFO)
export GENESIS_UTXO="0000000000000000000000000000000000000000000000000000000000000000#0"

echo ""
echo "Generating chain-spec.json file for Midnight Nodes..."

# Create pc-chain-config.json with genesis_utxo and cardano_addresses
jq 'env as $env | . + {
  "chain_parameters": {
    "genesis_utxo": $env.GENESIS_UTXO
  },
  "cardano_addresses": {
    "committee_candidates_address": "addr_test1wr4zpkfvylru9y3zahezf6vvfz7hlhf2pa4h9vxq70xwqzszre3qk",
    "permissioned_candidates_policy_id": $env.PERMISSIONED_CANDIDATES_POLICY_ID
  }
}' res/local-environment/pc-chain-config.json > /tmp/pc-chain-config.json

# Create patched federated-authority-config.json with Aiken policy IDs and addresses
echo "Patching federated-authority-config.json with deployed Aiken contract values..."
echo "  Council policy ID: $COUNCIL_POLICY_ID"
echo "  Council address: $COUNCIL_SCRIPT_ADDRESS"
echo "  Tech-auth policy ID: $TECHAUTH_POLICY_ID"
echo "  Tech-auth address: $TECHAUTH_SCRIPT_ADDRESS"

jq --arg council_addr "$COUNCIL_SCRIPT_ADDRESS" \
   --arg council_policy "$COUNCIL_POLICY_ID" \
   --arg techauth_addr "$TECHAUTH_SCRIPT_ADDRESS" \
   --arg techauth_policy "$TECHAUTH_POLICY_ID" \
   '.council.address = $council_addr | .council.policy_id = $council_policy | .technical_committee.address = $techauth_addr | .technical_committee.policy_id = $techauth_policy' \
   /res/dev/federated-authority-config.json > /tmp/federated-authority-config.json

echo "Patched federated-authority-config.json:"
cat /tmp/federated-authority-config.json

# Patch system-parameters-config.json to use the same D-parameter values as deployed on Cardano.
# This ensures the genesis D-parameter matches what was deployed, avoiding finality issues during
# the initial epochs before the on-chain D-parameter propagates to the sidechain.
echo "Patching system-parameters-config.json with D-parameter values..."
jq --argjson d_perm "$D_PERMISSIONED" --argjson d_reg "$D_REGISTERED" \
   '.d_parameter.num_permissioned_candidates = $d_perm | .d_parameter.num_registered_candidates = $d_reg' \
   /res/dev/system-parameters-config.json > /tmp/system-parameters-config.json

echo "Patched system-parameters-config.json:"
cat /tmp/system-parameters-config.json

# Create permissioned-candidates-config.json with deployed Aiken policy ID and first D_PERMISSIONED candidates
echo "Creating permissioned-candidates-config.json with deployed Aiken policy ID..."
jq --arg policy_id "$PERMISSIONED_CANDIDATES_POLICY_ID" --argjson d_perm "$D_PERMISSIONED" \
   '.permissioned_candidates_policy_id = ("0x" + $policy_id) | .initial_permissioned_candidates = .initial_permissioned_candidates[:$d_perm]' \
   /midnight-setup/permissioned-candidates-config.json > /tmp/permissioned-candidates-config.json

echo "Created permissioned-candidates-config.json:"
cat /tmp/permissioned-candidates-config.json

# Create registered-candidates-addresses.json
echo "Creating registered-candidates-addresses.json..."
cat <<EOF > /tmp/registered-candidates-addresses.json
{
    "committee_candidates_address": "addr_test1wr4zpkfvylru9y3zahezf6vvfz7hlhf2pa4h9vxq70xwqzszre3qk"
}
EOF

echo "Created registered-candidates-addresses.json:"
cat /tmp/registered-candidates-addresses.json


echo "Creating cnight-config.json..."
jq '.observed_utxos.end = .observed_utxos.start
  | .observed_utxos.utxos = []
  | .mappings = {}
  | .utxo_owners = {}
  | .next_cardano_position = .observed_utxos.start
  | .system_tx = null' res/local-environment/cnight-config.json > /tmp/cnight-config.json

echo "Created cnight-config.json:"
cat /tmp/cnight-config.json

export CHAINSPEC_NAME=localenv1
export CHAINSPEC_ID=localenv
export CHAINSPEC_NETWORK_ID=devnet
export CHAINSPEC_GENESIS_STATE=res/genesis/genesis_state_undeployed.mn
export CHAINSPEC_GENESIS_BLOCK=res/genesis/genesis_block_undeployed.mn
export CHAINSPEC_GENESIS_TX=res/genesis/genesis_tx_undeployed.mn  #  0.13.5 compatibility, can be removed in the future
export CHAINSPEC_CHAIN_TYPE=live
export CHAINSPEC_PC_CHAIN_CONFIG=/tmp/pc-chain-config.json
export CHAINSPEC_CNIGHT_GENESIS=/tmp/cnight-config.json
export CHAINSPEC_ICS_CONFIG=res/local-environment/ics-config.json
export CHAINSPEC_RESERVE_CONFIG=res/local-environment/reserve-config.json
export CHAINSPEC_FEDERATED_AUTHORITY_CONFIG=/tmp/federated-authority-config.json
export CHAINSPEC_SYSTEM_PARAMETERS_CONFIG=/tmp/system-parameters-config.json
export CHAINSPEC_PERMISSIONED_CANDIDATES_CONFIG=/tmp/permissioned-candidates-config.json
export CHAINSPEC_REGISTERED_CANDIDATES_ADDRESSES=/tmp/registered-candidates-addresses.json

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

echo "Waiting 2 epochs for DParam to become active and contracts to be queryable..."
n_2_epoch=$((epoch + 2))
echo "Current epoch: $epoch"
while [ "$epoch" -lt $n_2_epoch ]; do
  sleep 10
  epoch=$(curl -s --request POST \
    --url "http://ogmios:1337" \
    --header 'Content-Type: application/json' \
    --data '{"jsonrpc": "2.0", "method": "queryLedgerState/epoch"}' | jq .result)
  echo "Current epoch: $epoch"
done
echo "DParam is now active!"
