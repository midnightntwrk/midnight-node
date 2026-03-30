#!/bin/bash

# This file is part of midnight-node.
# Copyright (C) Midnight Foundation
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

# If contracts were already compiled and deployed, skip everything.
if [ -f /runtime-values/contracts-active-epoch ]; then
  echo "contracts-active-epoch already exists, skipping contract compilation and deployment."
  exit 0
fi

echo "=== Governance Contract Compiler and Deployer ==="

RUNTIME_VALUES="/runtime-values"
CONTRACTS_SRC="/contracts"
CONTRACTS_DIR="/tmp/contracts"
OUTPUT_DIR="/runtime-values"
AIKEN_TOML="${CONTRACTS_DIR}/aiken.toml"
PLUTUS_JSON="${CONTRACTS_DIR}/plutus-local.json"

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


# Update aiken config for local environment
echo "=== Aiken Config Update ==="
toml set aiken.toml config.local.reserve_one_shot_hash.bytes "$one_shot_hash" > aiken.toml.tmp && mv aiken.toml.tmp aiken.toml
toml set aiken.toml config.local.council_one_shot_hash.bytes "$one_shot_hash" > aiken.toml.tmp && mv aiken.toml.tmp aiken.toml
toml set aiken.toml config.local.ics_one_shot_hash.bytes "$one_shot_hash" > aiken.toml.tmp && mv aiken.toml.tmp aiken.toml
toml set aiken.toml config.local.technical_authority_one_shot_hash.bytes "$one_shot_hash" > aiken.toml.tmp && mv aiken.toml.tmp aiken.toml
toml set aiken.toml config.local.federated_operators_one_shot_hash.bytes "$one_shot_hash" > aiken.toml.tmp && mv aiken.toml.tmp aiken.toml
toml set aiken.toml config.local.main_gov_one_shot_hash.bytes "$one_shot_hash" > aiken.toml.tmp && mv aiken.toml.tmp aiken.toml
toml set aiken.toml config.local.staging_gov_one_shot_hash.bytes "$one_shot_hash" > aiken.toml.tmp && mv aiken.toml.tmp aiken.toml
toml set aiken.toml config.local.main_council_update_one_shot_hash.bytes "$one_shot_hash" > aiken.toml.tmp && mv aiken.toml.tmp aiken.toml
toml set aiken.toml config.local.main_tech_auth_update_one_shot_hash.bytes "$one_shot_hash" > aiken.toml.tmp && mv aiken.toml.tmp aiken.toml
toml set aiken.toml config.local.main_federated_ops_update_one_shot_hash.bytes "$one_shot_hash" > aiken.toml.tmp && mv aiken.toml.tmp aiken.toml
toml set aiken.toml config.local.committee_bridge_one_shot_hash.bytes "$one_shot_hash" > aiken.toml.tmp && mv aiken.toml.tmp aiken.toml
toml set aiken.toml config.local.committee_threshold_one_shot_hash.bytes "$one_shot_hash" > aiken.toml.tmp && mv aiken.toml.tmp aiken.toml
toml set aiken.toml config.local.terms_and_conditions_one_shot_hash.bytes "$one_shot_hash" > aiken.toml.tmp && mv aiken.toml.tmp aiken.toml
toml set aiken.toml config.local.terms_and_conditions_threshold_one_shot_hash.bytes "$one_shot_hash" > aiken.toml.tmp && mv aiken.toml.tmp aiken.toml
toml set aiken.toml config.local.cnight_minting_one_shot_hash.bytes "$one_shot_hash" > aiken.toml.tmp && mv aiken.toml.tmp aiken.toml
toml set aiken.toml config.local.collateral_utxo_hash.bytes "$one_shot_hash" > aiken.toml.tmp && mv aiken.toml.tmp aiken.toml

# Debug: Show the updated local section of aiken.toml
echo "--- aiken.toml config.local values ---"
toml get aiken.toml config.local | jq -r
echo ""

echo "✓ Aiken config updated successfully"
echo "==================================="
echo ""


# Compile contracts with local config
echo "=== Aiken Contracts Compilation ==="
echo "Aiken version:"
aiken --version

echo "Compiling Aiken contracts with local config..."
just build local

# Check if plutus.json was generated
if [[ ! -f "${PLUTUS_JSON}" ]]; then
    echo "ERROR: plutus-local.json not generated after compilation"
    exit 1
fi

echo "✓ Contracts compiled successfully"
echo "=== Contracts Compilation Complete ==="
echo ""

# Deploy contracts
echo "=== Contracts Deployment ==="
bun cli deploy -p kupmios
bun cli sign-and-submit -p kupmios deployments/local/deployment-transactions.json

bun cli register-gov-auth -p kupmios --use-build
bun cli sign-and-submit -p kupmios deployments/local/register-gov-auth-tx.json

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
cp $PLUTUS_JSON $AIKEN_TOML ${CONTRACTS_DIR}/contract_blueprint.ts ${CONTRACTS_DIR}/contract_blueprint_local.ts $CONTRACTS_DIR/contracts-info.json $OUTPUT_DIR
echo "Contract files in ${OUTPUT_DIR}:"
ls $OUTPUT_DIR
echo ""

echo "✓ Contracts data exported successfully"
echo "=== Contracts Data Export Complete ==="
echo ""

echo "=== Contract Compiler Complete ==="
