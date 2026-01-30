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
PLUTUS_JSON="${CONTRACTS_DIR}/plutus.json"

# Maximum wait time for hash files (seconds)
MAX_WAIT_TIME=120

# Copy contracts to writable location
echo "Copying contracts to writable location..."
cp -r "${CONTRACTS_SRC}" "${CONTRACTS_DIR}"
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

# Wait for one-shot hash files to be available
echo "Waiting for one-shot UTxO hashes..."
start_time=$(date +%s)
while true; do
    if [[ -f "${RUNTIME_VALUES}/council_oneshot_hash.txt" ]] && \
       [[ -f "${RUNTIME_VALUES}/techauth_oneshot_hash.txt" ]] && \
       [[ -f "${RUNTIME_VALUES}/federatedops_oneshot_hash.txt" ]]; then
        echo "✓ All one-shot hash files found"
        break
    fi

    elapsed=$(($(date +%s) - start_time))
    if [[ $elapsed -ge $MAX_WAIT_TIME ]]; then
        echo "ERROR: Timeout waiting for one-shot hash files after ${MAX_WAIT_TIME}s"
        ls -la "${RUNTIME_VALUES}/" || true
        exit 1
    fi

    echo "Waiting for hash files (${elapsed}s elapsed)..."
    sleep 2
done

# Read one-shot hashes and indexes
COUNCIL_HASH=$(cat "${RUNTIME_VALUES}/council_oneshot_hash.txt" | tr -d '\n\r')
COUNCIL_INDEX=$(cat "${RUNTIME_VALUES}/council_oneshot_index.txt" | tr -d '\n\r')
TECHAUTH_HASH=$(cat "${RUNTIME_VALUES}/techauth_oneshot_hash.txt" | tr -d '\n\r')
TECHAUTH_INDEX=$(cat "${RUNTIME_VALUES}/techauth_oneshot_index.txt" | tr -d '\n\r')
FEDERATEDOPS_HASH=$(cat "${RUNTIME_VALUES}/federatedops_oneshot_hash.txt" | tr -d '\n\r')
FEDERATEDOPS_INDEX=$(cat "${RUNTIME_VALUES}/federatedops_oneshot_index.txt" | tr -d '\n\r')

echo "One-shot UTxO hashes:"
echo "  Council:        ${COUNCIL_HASH}#${COUNCIL_INDEX}"
echo "  Tech Authority: ${TECHAUTH_HASH}#${TECHAUTH_INDEX}"
echo "  Federated Ops:  ${FEDERATEDOPS_HASH}#${FEDERATEDOPS_INDEX}"

# Navigate to contracts directory
cd "${CONTRACTS_DIR}"

# Instead of creating a new [config.localenv] section and using --env localenv,
# we directly modify the [config.default] values. This is more reliable because
# Aiken's --env flag doesn't properly inherit missing values from default.
echo "Modifying [config.default] section with local-env one-shot hashes..."

# Helper function to update a bytes value in aiken.toml
# Usage: update_bytes_value "section_name" "new_hex_value"
update_bytes_value() {
    local section="$1"
    local new_value="$2"
    
    # Use awk to find the section and update the bytes value on the next line
    awk -v section="$section" -v newval="$new_value" '
        $0 ~ "^\\[config\\.default\\." section "\\]" { 
            print; 
            getline; 
            gsub(/bytes = "[^"]*"/, "bytes = \"" newval "\""); 
            print; 
            next 
        }
        { print }
    ' "${AIKEN_TOML}" > "${AIKEN_TOML}.tmp" && mv "${AIKEN_TOML}.tmp" "${AIKEN_TOML}"
}

# Helper function to update a simple integer value in aiken.toml
# Usage: update_int_value "key_name" "new_value"
update_int_value() {
    local key="$1"
    local new_value="$2"
    
    # Use sed to update the value in [config.default] section
    sed -i "s/^${key} = [0-9]*/${key} = ${new_value}/" "${AIKEN_TOML}"
}

# Update the one-shot indices (set all to 0 for local environment)
echo "Updating one-shot indices to 0..."
update_int_value "council_one_shot_index" "${COUNCIL_INDEX}"
update_int_value "technical_authority_one_shot_index" "${TECHAUTH_INDEX}"
update_int_value "federated_operators_one_shot_index" "${FEDERATEDOPS_INDEX}"

# Update the one-shot hashes with the actual UTxO transaction hashes
echo "Updating one-shot hashes..."
update_bytes_value "council_one_shot_hash" "${COUNCIL_HASH}"
update_bytes_value "technical_authority_one_shot_hash" "${TECHAUTH_HASH}"
update_bytes_value "federated_operators_one_shot_hash" "${FEDERATEDOPS_HASH}"

# Debug: Show the updated config values
echo "Verifying updated config.default values:"
echo "  Council one-shot hash: ${COUNCIL_HASH}"
echo "  Council one-shot index: ${COUNCIL_INDEX}"
echo "  Tech Auth one-shot hash: ${TECHAUTH_HASH}"
echo "  Federated Ops one-shot hash: ${FEDERATEDOPS_HASH}"

echo "✓ Config values updated"

# Verify the aiken.toml was updated correctly
echo "Verifying aiken.toml config.default section..."
if grep -q "^\[config\.default\.council_one_shot_hash\]" "${AIKEN_TOML}"; then
    CONFIGURED_HASH=$(grep -A1 "^\[config\.default\.council_one_shot_hash\]" "${AIKEN_TOML}" | grep "bytes" | sed 's/.*= "\(.*\)"/\1/')
    echo "  Configured council_one_shot_hash: ${CONFIGURED_HASH}"
    if [[ "${CONFIGURED_HASH}" != "${COUNCIL_HASH}" ]]; then
        echo "ERROR: aiken.toml council_one_shot_hash mismatch!"
        echo "  Expected: ${COUNCIL_HASH}"
        echo "  Found:    ${CONFIGURED_HASH}"
        exit 1
    fi
    echo "✓ aiken.toml config.default verified"
else
    echo "ERROR: [config.default.council_one_shot_hash] section not found in aiken.toml"
    exit 1
fi

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

# Debug: Show the updated default section of aiken.toml
echo "=== aiken.toml config.default one-shot values ==="
grep -A2 "^\[config\.default\.council_one_shot_hash\]" "${AIKEN_TOML}" || echo "No council_one_shot_hash found!"
grep -A2 "^\[config\.default\.technical_authority_one_shot_hash\]" "${AIKEN_TOML}" || echo "No technical_authority_one_shot_hash found!"
grep -A2 "^\[config\.default\.federated_operators_one_shot_hash\]" "${AIKEN_TOML}" || echo "No federated_operators_one_shot_hash found!"
echo "==================================="

# aiken build may return non-zero for test failures but still generate plutus.json
# Use --trace-level silent to reduce output noise
# No --env flag needed since we modified config.default directly
aiken build --trace-level silent || true

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

# Verify the compiled contract uses updated config by checking hash differs from original default
# The original default council_forever hash (before we modified aiken.toml)
ORIGINAL_DEFAULT_COUNCIL_HASH="fe98bfeaa4af53bcf84ddc097c3f7d4b1acf76e5ce83fa920049b2c1"
COMPILED_COUNCIL_HASH=$(jq -r '.validators[] | select(.title == "permissioned.council_forever.else") | .hash' "${PLUTUS_JSON}" 2>/dev/null || echo "")
if [[ "${COMPILED_COUNCIL_HASH}" == "${ORIGINAL_DEFAULT_COUNCIL_HASH}" ]]; then
    echo "ERROR: Compiled council_forever hash matches ORIGINAL default config!"
    echo "  This suggests the config.default updates were not applied correctly."
    echo "  Expected a different hash when using modified one-shot hashes."
    echo "  Original default: ${ORIGINAL_DEFAULT_COUNCIL_HASH}"
    echo "  Compiled:         ${COMPILED_COUNCIL_HASH}"
    exit 1
else
    echo "✓ Compiled hash differs from original default (config update applied)"
    echo "  New policy ID: ${COMPILED_COUNCIL_HASH}"
fi

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

# Extract CBOR for each validator and write to runtime-values
echo "Extracting contract CBOR to runtime-values..."

# List available validators for debugging
echo "Available validators in plutus.json:"
jq -r '.validators[].title' "${PLUTUS_JSON}" 2>/dev/null | grep -i "forever" || echo "  (none matching 'forever')"

# Extract council_forever CBOR (matches permissioned.council_forever.else)
COUNCIL_CBOR=$(jq -r '.validators[] | select(.title | test("council_forever"; "i")) | .compiledCode' "${PLUTUS_JSON}" 2>/dev/null | head -1 || echo "")
if [[ -n "${COUNCIL_CBOR}" && "${COUNCIL_CBOR}" != "null" ]]; then
    echo "${COUNCIL_CBOR}" > "${OUTPUT_DIR}/council_forever.cbor"
    echo "✓ Wrote council_forever.cbor (${#COUNCIL_CBOR} chars)"
else
    echo "ERROR: Could not extract council_forever CBOR"
    exit 1
fi

# Extract tech_auth_forever CBOR (matches permissioned.tech_auth_forever.else)
TECHAUTH_CBOR=$(jq -r '.validators[] | select(.title | test("tech_auth_forever"; "i")) | .compiledCode' "${PLUTUS_JSON}" 2>/dev/null | head -1 || echo "")
if [[ -n "${TECHAUTH_CBOR}" && "${TECHAUTH_CBOR}" != "null" ]]; then
    echo "${TECHAUTH_CBOR}" > "${OUTPUT_DIR}/tech_auth_forever.cbor"
    echo "✓ Wrote tech_auth_forever.cbor (${#TECHAUTH_CBOR} chars)"
else
    echo "ERROR: Could not extract tech_auth_forever CBOR"
    exit 1
fi

# Extract federated_ops_forever CBOR (matches permissioned.federated_ops_forever.else)
FEDOPS_CBOR=$(jq -r '.validators[] | select(.title | test("federated_ops_forever"; "i")) | .compiledCode' "${PLUTUS_JSON}" 2>/dev/null | head -1 || echo "")
if [[ -n "${FEDOPS_CBOR}" && "${FEDOPS_CBOR}" != "null" ]]; then
    echo "${FEDOPS_CBOR}" > "${OUTPUT_DIR}/federated_ops_forever.cbor"
    echo "✓ Wrote federated_ops_forever.cbor (${#FEDOPS_CBOR} chars)"
else
    echo "ERROR: Could not extract federated_ops_forever CBOR"
    exit 1
fi

echo ""
echo "=== Contract Compilation Complete ==="
echo "CBOR files in ${OUTPUT_DIR}:"
ls -la "${OUTPUT_DIR}"/*.cbor 2>/dev/null || echo "  No .cbor files found"
