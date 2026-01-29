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
# This script reads one-shot hashes from runtime-values and compiles contracts

set -euo pipefail

echo "=== Governance Contract Compiler ==="

RUNTIME_VALUES="/runtime-values"
CONTRACTS_SRC="/contracts"
CONTRACTS_DIR="/tmp/contracts"
OUTPUT_DIR="/runtime-values"
AIKEN_TOML="${CONTRACTS_DIR}/aiken.toml"
PLUTUS_JSON="${CONTRACTS_DIR}/plutus-default.json"

# Maximum wait time for hash files (seconds)
MAX_WAIT_TIME=120

# Copy contracts to writable location
echo "Copying contracts to writable location..."
cp -r "${CONTRACTS_SRC}" "${CONTRACTS_DIR}"
cp /.env $CONTRACTS_DIR
echo "✓ Contracts copied to ${CONTRACTS_DIR}"

# Clean any existing build artifacts to ensure fresh compilation
if [[ -d "${CONTRACTS_DIR}/build" ]]; then
    echo "Removing existing build directory..."
    rm -rf "${CONTRACTS_DIR}/build"
    echo "✓ Build directory cleaned"
fi

# Navigate to contracts directory
cd "${CONTRACTS_DIR}"

# Deploy contracts
bun cli simple-tx -p kupmios
bun cli sign-and-submit -p kupmios deployments/local/simple-tx.json
one_shot_hash=$(jq -r '.txHash' deployments/local/simple-tx.json)
echo "One shot hash: $one_shot_hash"

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

# toml set aiken.toml config.default.reserve_one_shot_index 0 > aiken.toml.tmp && mv aiken.toml.tmp aiken.toml
# toml set aiken.toml config.default.council_one_shot_index 1 > aiken.toml.tmp && mv aiken.toml.tmp aiken.toml
# toml set aiken.toml config.default.ics_one_shot_index 2 > aiken.toml.tmp && mv aiken.toml.tmp aiken.toml
# toml set aiken.toml config.default.technical_authority_one_shot_index 3 > aiken.toml.tmp && mv aiken.toml.tmp aiken.toml
# toml set aiken.toml config.default.federated_operators_one_shot_index 4 > aiken.toml.tmp && mv aiken.toml.tmp aiken.toml
# toml set aiken.toml config.default.main_gov_one_shot_index 5 > aiken.toml.tmp && mv aiken.toml.tmp aiken.toml
# toml set aiken.toml config.default.staging_gov_one_shot_index 6 > aiken.toml.tmp && mv aiken.toml.tmp aiken.toml
# toml set aiken.toml config.default.main_council_update_one_shot_index 7 > aiken.toml.tmp && mv aiken.toml.tmp aiken.toml
# toml set aiken.toml config.default.main_tech_auth_update_one_shot_index 8 > aiken.toml.tmp && mv aiken.toml.tmp aiken.toml
# toml set aiken.toml config.default.main_federated_ops_update_one_shot_index 9 > aiken.toml.tmp && mv aiken.toml.tmp aiken.toml
# toml set aiken.toml config.default.committee_bridge_one_shot_index 10 > aiken.toml.tmp && mv aiken.toml.tmp aiken.toml
# toml set aiken.toml config.default.committee_threshold_one_shot_index 11 > aiken.toml.tmp && mv aiken.toml.tmp aiken.toml
# toml set aiken.toml config.default.terms_and_conditions_one_shot_index 12 > aiken.toml.tmp && mv aiken.toml.tmp aiken.toml
# toml set aiken.toml config.default.terms_and_conditions_threshold_one_shot_index 13 > aiken.toml.tmp && mv aiken.toml.tmp aiken.toml
# toml set aiken.toml config.default.cnight_minting_one_shot_index 14 > aiken.toml.tmp && mv aiken.toml.tmp aiken.toml
# toml set aiken.toml config.default.collateral_utxo_index 15 > aiken.toml.tmp && mv aiken.toml.tmp aiken.toml

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

toml get aiken.toml config.default | jq -r


# Compile contracts using aiken directly with modified default config
# Note: We don't use build_contracts.sh as it requires toml-cli and does
# multi-stage compilation. For forever contracts, a simple aiken build suffices.
# We modify [config.default] directly instead of using --env because Aiken's
# environment inheritance doesn't work as expected for our use case.
echo "Compiling Aiken contracts with modified default config..."

# Show Aiken version for debugging
echo "Aiken version:"
aiken --version

# Clean build directory to ensure no stale artifacts
rm -rf build/

# Install dependencies, compile and deploy
bun install
just build
bun cli deploy -p kupmios
bun cli sign-and-submit -p kupmios deployments/local/deployment-transactions.json

# Debug: Show the updated default section of aiken.toml
echo "=== aiken.toml config.default one-shot values ==="
toml get $AIKEN_TOML config.default.council_one_shot_hash || echo "No council_one_shot_hash found!"
toml get $AIKEN_TOML config.default.technical_authority_one_shot_hash || echo "No technical_authority_one_shot_hash found!"
toml get $AIKEN_TOML config.default.federated_operators_one_shot_hash || echo "No federated_operators_one_shot_hash found!"
echo "==================================="

# Check if plutus.json was generated
if [[ ! -f "${PLUTUS_JSON}" ]]; then
    echo "ERROR: plutus.json not generated after compilation"
    exit 1
fi

echo "✓ Contracts compiled successfully"

# Debug: Show compiled policy IDs to verify localenv config was applied
echo "Compiled validator hashes:"
echo "  council_forever: $(jq -r '.validators[] | select(.title | contains("council_forever")) | .hash' "${PLUTUS_JSON}" 2>/dev/null || echo "not found")"
echo "  tech_auth_forever: $(jq -r '.validators[] | select(.title | contains("tech_auth_forever")) | .hash' "${PLUTUS_JSON}" 2>/dev/null || echo "not found")"
echo "  federated_ops_forever: $(jq -r '.validators[] | select(.title | contains("federated_ops_forever")) | .hash' "${PLUTUS_JSON}" 2>/dev/null || echo "not found")"

# Write policy IDs to runtime-values for use in chain-spec generation
echo "Writing Aiken policy IDs to runtime-values..."
COUNCIL_POLICY_ID=$(jq -r '.validators[] | select(.title | test("council_forever"; "i")) | .hash' "${PLUTUS_JSON}" 2>/dev/null | head -1 || echo "")
TECHAUTH_POLICY_ID=$(jq -r '.validators[] | select(.title | test("tech_auth_forever"; "i")) | .hash' "${PLUTUS_JSON}" 2>/dev/null | head -1 || echo "")
FEDOPS_POLICY_ID=$(jq -r '.validators[] | select(.title | test("federated_ops_forever"; "i")) | .hash' "${PLUTUS_JSON}" 2>/dev/null | head -1 || echo "")

if [[ -n "${COUNCIL_POLICY_ID}" && "${COUNCIL_POLICY_ID}" != "null" ]]; then
    echo "${COUNCIL_POLICY_ID}" > "${OUTPUT_DIR}/council_forever_policy_id.txt"
    echo "✓ Wrote council_forever_policy_id.txt: ${COUNCIL_POLICY_ID}"
else
    echo "ERROR: Could not extract council_forever policy ID"
    exit 1
fi

if [[ -n "${TECHAUTH_POLICY_ID}" && "${TECHAUTH_POLICY_ID}" != "null" ]]; then
    echo "${TECHAUTH_POLICY_ID}" > "${OUTPUT_DIR}/tech_auth_forever_policy_id.txt"
    echo "✓ Wrote tech_auth_forever_policy_id.txt: ${TECHAUTH_POLICY_ID}"
fi

if [[ -n "${FEDOPS_POLICY_ID}" && "${FEDOPS_POLICY_ID}" != "null" ]]; then
    echo "${FEDOPS_POLICY_ID}" > "${OUTPUT_DIR}/federated_ops_forever_policy_id.txt"
    echo "✓ Wrote federated_ops_forever_policy_id.txt: ${FEDOPS_POLICY_ID}"
fi


# List available validators for debugging
echo "Available validators in plutus.json:"
jq -r '.validators[].title' "${PLUTUS_JSON}" 2>/dev/null | grep -i "forever" || echo "  (none matching 'forever')"


echo ""
echo "=== Contract Compilation Complete ==="

# Export all contract data for midnight-setup
bun cli info --format json > $CONTRACTS_DIR/contracts-info.json
cp $PLUTUS_JSON $AIKEN_TOML ${CONTRACTS_DIR}/contract_blueprint.ts ${CONTRACTS_DIR}/contract_blueprint_default.ts $CONTRACTS_DIR/contracts-info.json $OUTPUT_DIR
echo "Contract files in ${OUTPUT_DIR}:"
ls $OUTPUT_DIR
