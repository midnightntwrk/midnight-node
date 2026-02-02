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

export GENESIS_UTXO="0000000000000000000000000000000000000000000000000000000000000000#0"
cat /shared/genesis.utxo
echo "Genesis UTXO: $GENESIS_UTXO"


# export MOCK_REGISTRATIONS_FILE="/node-dev/default-registrations.json"
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


echo "Inserting D parameter..."

# D_PERMISSIONED + D_REGISTERED must be >= 5 for a functioning partner chains network.
# Using 3 permissioned (Alice, Bob, Charlie from qanet config).
# This ensures GRANDPA finality works since nodes 1-3 use well-known keys matching qanet config.
D_PERMISSIONED=3
D_REGISTERED=0


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
       [[ -f "${RUNTIME_VALUES}/federated_ops_forever.cbor" ]] && \
       [[ -f "${RUNTIME_VALUES}/council_forever_policy_id.txt" ]]; then
        echo "✓ All contract CBOR files and policy IDs found"
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

# Override PERMISSIONED_CANDIDATES_POLICY_ID with the Aiken federated_ops_forever policy ID
# This ensures the chain-spec uses the Aiken contract policy ID for permissioned candidates
AIKEN_PERMISSIONED_CANDIDATES_POLICY_ID=$(cat "${RUNTIME_VALUES}/federated_ops_forever_policy_id.txt")
export PERMISSIONED_CANDIDATES_POLICY_ID="$AIKEN_PERMISSIONED_CANDIDATES_POLICY_ID"

# Get the funded address from the shared volume
FUNDED_ADDRESS=$(cat /shared/FUNDED_ADDRESS)
echo "Using funded address: $FUNDED_ADDRESS"

# Read Sr25519 (aura) keys from midnight nodes for council/tech-auth contracts
# Note: council/tech-auth use Sr25519 keys (32 bytes), NOT ECDSA sidechain keys (33 bytes)
alice_aura_vkey=$(cat /midnight-nodes/midnight-node-1/keys/aura.vkey)
bob_aura_vkey=$(cat /midnight-nodes/midnight-node-2/keys/aura.vkey)
charlie_aura_vkey=$(cat /midnight-nodes/midnight-node-3/keys/aura.vkey)

# Use deterministic Cardano key hashes for testing (28 bytes each)
# These are test values that match the format used in E2E tests
alice_cardano_hash="e8c300330fe315531ca89d4a2e7d0c80211bc70b473b1ed4979dff2a"
bob_cardano_hash="e8c300330fe315531ca89d4a2e7d0c80211bc70b473b1ed4979dff2b"
charlie_cardano_hash="e8c300330fe315531ca89d4a2e7d0c80211bc70b473b1ed4979dff2c"

# Create members.json for council_forever contract
# Uses Sr25519 (aura) keys which are 32 bytes
cat <<EOF > council_members.json
[
  {"cardano_hash": "$alice_cardano_hash", "sr25519_key": "$alice_aura_vkey"},
  {"cardano_hash": "$bob_cardano_hash", "sr25519_key": "$bob_aura_vkey"},
  {"cardano_hash": "$charlie_cardano_hash", "sr25519_key": "$charlie_aura_vkey"}
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

# Deploy council_forever contract
echo ""
echo "=== Deploying Council Forever Contract ==="
COUNCIL_OUTPUT_FILE=/tmp/council_deploy_output.txt
./aiken-deployer \
    --contract-cbor "${RUNTIME_VALUES}/council_forever.cbor" \
    --one-shot-utxo "${COUNCIL_ONESHOT_HASH}#${COUNCIL_ONESHOT_INDEX}" \
    --signing-key /tmp/signing_key.cbor \
    --funded-address "$FUNDED_ADDRESS" \
    --members-file council_members.json \
    --ogmios-url "$OGMIOS_URL" 2>&1 | tee "$COUNCIL_OUTPUT_FILE"
COUNCIL_EXIT_CODE=${PIPESTATUS[0]}

if [ "$COUNCIL_EXIT_CODE" -eq 0 ]; then
    echo "✓ Council Forever contract deployed successfully!"
    # Parse policy ID and script address from output
    COUNCIL_POLICY_ID=$(grep "Policy ID:" "$COUNCIL_OUTPUT_FILE" | head -1 | awk '{print $3}')
    COUNCIL_SCRIPT_ADDRESS=$(grep "Script address:" "$COUNCIL_OUTPUT_FILE" | head -1 | awk '{print $3}')
    echo "  Captured council policy ID: $COUNCIL_POLICY_ID"
    echo "  Captured council script address: $COUNCIL_SCRIPT_ADDRESS"
else
    echo "✗ Council Forever contract deployment failed"
    cat "$COUNCIL_OUTPUT_FILE"
    exit 1
fi

# Wait for transaction to confirm
sleep 10

# Deploy tech_auth_forever contract (uses same members for testing)
echo ""
echo "=== Deploying Tech Auth Forever Contract ==="
TECHAUTH_OUTPUT_FILE=/tmp/techauth_deploy_output.txt
./aiken-deployer \
    --contract-cbor "${RUNTIME_VALUES}/tech_auth_forever.cbor" \
    --one-shot-utxo "${TECHAUTH_ONESHOT_HASH}#${TECHAUTH_ONESHOT_INDEX}" \
    --signing-key /tmp/signing_key.cbor \
    --funded-address "$FUNDED_ADDRESS" \
    --members-file council_members.json \
    --ogmios-url "$OGMIOS_URL" \
    --contract-type tech-auth 2>&1 | tee "$TECHAUTH_OUTPUT_FILE"
TECHAUTH_EXIT_CODE=${PIPESTATUS[0]}

if [ "$TECHAUTH_EXIT_CODE" -eq 0 ]; then
    echo "✓ Tech Auth Forever contract deployed successfully!"
    # Parse policy ID and script address from output
    TECHAUTH_POLICY_ID=$(grep "Policy ID:" "$TECHAUTH_OUTPUT_FILE" | head -1 | awk '{print $3}')
    TECHAUTH_SCRIPT_ADDRESS=$(grep "Script address:" "$TECHAUTH_OUTPUT_FILE" | head -1 | awk '{print $3}')
    echo "  Captured tech-auth policy ID: $TECHAUTH_POLICY_ID"
    echo "  Captured tech-auth script address: $TECHAUTH_SCRIPT_ADDRESS"
else
    echo "✗ Tech Auth Forever contract deployment failed"
    cat "$TECHAUTH_OUTPUT_FILE"
    exit 1
fi

# Wait for transaction to confirm
sleep 10

# Generate permissioned candidates file for federated_ops_forever
# Extract the first 3 candidates from the chain config (matching D_PERMISSIONED)
echo ""
echo "=== Generating Permissioned Candidates File ==="
jq '[.initial_permissioned_candidates[:3] | .[] | {
    ecdsa_key: .sidechain_pub_key[2:],
    aura_key: .aura_pub_key[2:],
    grandpa_key: .grandpa_pub_key[2:]
}]' res/qanet/pc-chain-config.json > permissioned_candidates.json
echo "Created permissioned_candidates.json:"
cat permissioned_candidates.json

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
    --contract-type federated-ops \
    --candidates-file permissioned_candidates.json

if [ "$?" -eq 0 ]; then
    echo "✓ Federated Ops Forever contract deployed successfully!"
else
    echo "✗ Federated Ops Forever contract deployment failed"
    exit 1
fi

# The FederatedOps policy ID is automatically used via PERMISSIONED_CANDIDATES_POLICY_ID
# which was overridden earlier and will be included in pc-chain-config.json

echo ""
echo "=== All Governance Contracts Deployed Successfully ==="

echo "Generating chain-spec.json file for Midnight Nodes..."

cat res/qanet/pc-chain-config.json | jq '.initial_permissioned_candidates |= .[:4]' > /tmp/pc-chain-config-qanet.json

jq 'env as $env | . + {
  "chain_parameters": {
    "genesis_utxo": $env.GENESIS_UTXO
  },
  "cardano_addresses": {
    "committee_candidates_address": "addr_test1wr4zpkfvylru9y3zahezf6vvfz7hlhf2pa4h9vxq70xwqzszre3qk",
    "permissioned_candidates_policy_id": $env.PERMISSIONED_CANDIDATES_POLICY_ID,
  }
}' /tmp/pc-chain-config-qanet.json > /tmp/pc-chain-config.json

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

export CHAINSPEC_NAME=localenv1
export CHAINSPEC_ID=localenv
export CHAINSPEC_NETWORK_ID=devnet
export CHAINSPEC_GENESIS_STATE=res/genesis/genesis_state_undeployed.mn
export CHAINSPEC_GENESIS_BLOCK=res/genesis/genesis_block_undeployed.mn
export CHAINSPEC_GENESIS_TX=res/genesis/genesis_tx_undeployed.mn  #  0.13.5 compatibility, can be removed in the future
export CHAINSPEC_CHAIN_TYPE=live
export CHAINSPEC_PC_CHAIN_CONFIG=/tmp/pc-chain-config.json
export CHAINSPEC_CNIGHT_GENESIS=res/qanet/cnight-genesis.json
export CHAINSPEC_FEDERATED_AUTHORITY_CONFIG=/tmp/federated-authority-config.json
export CHAINSPEC_SYSTEM_PARAMETERS_CONFIG=/tmp/system-parameters-config.json
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

echo "Waiting 3 epochs for DParam to become active and contracts to be queryable..."
echo "(SDK applies 2-epoch offset, so epoch 4 is needed to query data from epoch 2)"
epoch=$(curl -s --request POST \
    --url "http://ogmios:1337" \
    --header 'Content-Type: application/json' \
    --data '{"jsonrpc": "2.0", "method": "queryLedgerState/epoch"}' | jq .result)
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
