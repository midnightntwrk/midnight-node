#!/bin/bash

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

# Compile Aiken governance contracts with dynamic one-shot UTxO hashes

set -euo pipefail

echo "=== Governance Contract Compiler and Deployer ==="

RUNTIME_VALUES="/runtime-values"
CONTRACTS_SRC="/contracts"
CONTRACTS_DIR="/tmp/contracts"
OUTPUT_DIR="/runtime-values"
AIKEN_TOML="${CONTRACTS_DIR}/aiken.toml"
PLUTUS_JSON="${CONTRACTS_DIR}/plutus-default.json"

# Copy contracts to writable location
echo "Copying contracts to writable location..."
cp -r $CONTRACTS_SRC /tmp
cp /.env $CONTRACTS_DIR
echo "✓ Contracts copied to ${CONTRACTS_DIR}"

# Clean any existing build artifacts to ensure fresh compilation
if [[ -d "${CONTRACTS_DIR}/build" ]]; then
    echo "Removing existing build directory..."
    rm -rf "${CONTRACTS_DIR}/build"
    echo "✓ Build directory cleaned"
fi

# Remove any pre-built plutus.json from source repo to ensure fresh compilation
if [[ -f "${CONTRACTS_DIR}/plutus.json" ]]; then
    echo "Removing existing plutus.json..."
    rm -f "${CONTRACTS_DIR}/plutus.json"
    echo "✓ Existing plutus.json removed"
fi

# Navigate to contracts directory
cd "${CONTRACTS_DIR}"

echo "Installing node dependencies"
bun install


# Prepare one shot hash
echo "=== One Shot Hash Preparation ==="

bun cli simple-tx -p kupmios
bun cli sign-and-submit -p kupmios deployments/local/simple-tx.json
one_shot_hash=$(jq -r '.txHash' deployments/local/simple-tx.json)

echo "✓ One shot hash: $one_shot_hash generated successfully"
echo "==================================="
echo ""


# Update aiken config
echo "=== Aiken Config Update ==="
# Info: will use `toml set` but keeeping sed commands, just in case.
# sed -i '/\[config\.default\..*_one_shot_hash\]/,/^bytes = / s/^bytes = ".*"/bytes = "'"$one_shot_hash"'"/' aiken.toml
# sed -i '/\[config\.default\.collateral_utxo_hash\]/,/^bytes = / s/^bytes = ".*"/bytes = "'"$one_shot_hash"'"/' aiken.toml
toml set aiken.toml config.default.reserve_one_shot_hash.bytes "$one_shot_hash" > aiken.toml.tmp && mv aiken.toml.tmp aiken.toml
toml set aiken.toml config.default.council_one_shot_hash.bytes "$one_shot_hash" > aiken.toml.tmp && mv aiken.toml.tmp aiken.toml
toml set aiken.toml config.default.ics_one_shot_hash.bytes "$one_shot_hash" > aiken.toml.tmp && mv aiken.toml.tmp aiken.toml
toml set aiken.toml config.default.technical_authority_one_shot_hash.bytes "$one_shot_hash" > aiken.toml.tmp && mv aiken.toml.tmp aiken.toml
toml set aiken.toml config.default.federated_operators_one_shot_hash.bytes "$one_shot_hash" > aiken.toml.tmp && mv aiken.toml.tmp aiken.toml
toml set aiken.toml config.default.main_gov_one_shot_hash.bytes "$one_shot_hash" > aiken.toml.tmp && mv aiken.toml.tmp aiken.toml
toml set aiken.toml config.default.staging_gov_one_shot_hash.bytes "$one_shot_hash" > aiken.toml.tmp && mv aiken.toml.tmp aiken.toml
toml set aiken.toml config.default.main_council_update_one_shot_hash.bytes "$one_shot_hash" > aiken.toml.tmp && mv aiken.toml.tmp aiken.toml
toml set aiken.toml config.default.main_tech_auth_update_one_shot_hash.bytes "$one_shot_hash" > aiken.toml.tmp && mv aiken.toml.tmp aiken.toml
toml set aiken.toml config.default.main_federated_ops_update_one_shot_hash.bytes "$one_shot_hash" > aiken.toml.tmp && mv aiken.toml.tmp aiken.toml
toml set aiken.toml config.default.committee_bridge_one_shot_hash.bytes "$one_shot_hash" > aiken.toml.tmp && mv aiken.toml.tmp aiken.toml
toml set aiken.toml config.default.committee_threshold_one_shot_hash.bytes "$one_shot_hash" > aiken.toml.tmp && mv aiken.toml.tmp aiken.toml
toml set aiken.toml config.default.terms_and_conditions_one_shot_hash.bytes "$one_shot_hash" > aiken.toml.tmp && mv aiken.toml.tmp aiken.toml
toml set aiken.toml config.default.terms_and_conditions_threshold_one_shot_hash.bytes "$one_shot_hash" > aiken.toml.tmp && mv aiken.toml.tmp aiken.toml
toml set aiken.toml config.default.cnight_minting_one_shot_hash.bytes "$one_shot_hash" > aiken.toml.tmp && mv aiken.toml.tmp aiken.toml
toml set aiken.toml config.default.collateral_utxo_hash.bytes "$one_shot_hash" > aiken.toml.tmp && mv aiken.toml.tmp aiken.toml

# Info: toml set is adding "" which breaks cli
sed -i 's/^reserve_one_shot_index = .*/reserve_one_shot_index = 0/' aiken.toml
sed -i 's/^council_one_shot_index = .*/council_one_shot_index = 1/' aiken.toml
sed -i 's/^ics_one_shot_index = .*/ics_one_shot_index = 2/' aiken.toml
sed -i 's/^technical_authority_one_shot_index = .*/technical_authority_one_shot_index = 3/' aiken.toml
sed -i 's/^federated_operators_one_shot_index = .*/federated_operators_one_shot_index = 4/' aiken.toml
sed -i 's/^main_gov_one_shot_index = .*/main_gov_one_shot_index = 5/' aiken.toml
sed -i 's/^staging_gov_one_shot_index = .*/staging_gov_one_shot_index = 6/' aiken.toml
sed -i 's/^main_council_update_one_shot_index = .*/main_council_update_one_shot_index = 7/' aiken.toml
sed -i 's/^main_tech_auth_update_one_shot_index = .*/main_tech_auth_update_one_shot_index = 8/' aiken.toml
sed -i 's/^main_federated_ops_update_one_shot_index = .*/main_federated_ops_update_one_shot_index = 9/' aiken.toml
sed -i 's/^committee_bridge_one_shot_index = .*/committee_bridge_one_shot_index = 10/' aiken.toml
sed -i 's/^committee_threshold_one_shot_index = .*/committee_threshold_one_shot_index = 11/' aiken.toml
sed -i 's/^terms_and_conditions_one_shot_index = .*/terms_and_conditions_one_shot_index = 12/' aiken.toml
sed -i 's/^terms_and_conditions_threshold_one_shot_index = .*/terms_and_conditions_threshold_one_shot_index = 13/' aiken.toml
sed -i 's/^cnight_minting_one_shot_index = .*/cnight_minting_one_shot_index = 14/' aiken.toml
sed -i 's/^collateral_utxo_index = .*/collateral_utxo_index = 15/' aiken.toml

# Debug: Show the updated default section of aiken.toml
echo "--- aiken.toml config.default values ---"
toml get aiken.toml config.default | jq -r
echo ""

echo "✓ Aiken config updated successfully"
echo "==================================="
echo ""


# Compile contracts with modified default config
echo "=== Aiken Contracts Compilation ==="
echo "Aiken version:"
aiken --version

echo "Compiling Aiken contracts with modified default config..."
just build

# Check if plutus.json was generated
if [[ ! -f "${PLUTUS_JSON}" ]]; then
    echo "ERROR: plutus.json not generated after compilation"
    exit 1
fi

echo "✓ Contracts compiled successfully"
echo "=== Contracts Compilation Complete ==="
echo ""

# Deploy contracts
echo "=== Contracts Deployment ==="
bun cli deploy -p kupmios
bun cli sign-and-submit -p kupmios deployments/local/deployment-transactions.json

# TODO: uncomment when --use-build flag is added in contracts repo
# bun cli register-gov-auth -p kupmios --use-build
# bun cli sign-and-submit -p kupmios deployments/local/register-gov-auth-tx.json

echo "✓ Contracts deployed successfully"
echo "=== Contracts Deployment Complete ==="
echo ""

# Query current epoch from ogmios and save when contracts will be active
echo "=== Contracts Active Epoch ==="
epoch=$(curl -s --request POST \
    --url "http://ogmios:1337" \
    --header 'Content-Type: application/json' \
    --data '{"jsonrpc": "2.0", "method": "queryLedgerState/epoch"}' | jq .result)
contracts_active_epoch=$((epoch + 2))
echo "$contracts_active_epoch" > /runtime-values/contracts-active-epoch
echo "Current epoch: $epoch, contracts will be active at epoch: $contracts_active_epoch"
echo "=== Contracts Active Epoch Complete ==="
echo ""

# Export all contract data for midnight-setup
echo "=== Contracts Data Exporter ==="
echo "Saving contracts data for chain initialization (midnight-setup) and manual testing"
bun cli info --use-build --format json > $CONTRACTS_DIR/contracts-info.json
cp $PLUTUS_JSON $AIKEN_TOML ${CONTRACTS_DIR}/contract_blueprint.ts ${CONTRACTS_DIR}/contract_blueprint_default.ts $CONTRACTS_DIR/contracts-info.json $OUTPUT_DIR
echo "Contract files in ${OUTPUT_DIR}:"
ls $OUTPUT_DIR
echo ""

echo "✓ Contracts data exported successfully"
echo "=== Contracts Data Export Complete ==="
echo ""

# Signal completion for dependent services (healthcheck)
touch /tmp/ready

# Keep container alive for debugging
echo "=== Container Ready ==="
echo "Container will stay alive for debugging."
echo "To exec into this container, run:"
echo "  docker exec -it contract-compiler bash"
echo ""
sleep infinity
