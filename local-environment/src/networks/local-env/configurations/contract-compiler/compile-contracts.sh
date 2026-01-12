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

# Check if [config.localenv] already exists and remove it
if grep -q '^\[config\.localenv' "${AIKEN_TOML}"; then
    echo "Removing existing [config.localenv] sections..."
    # Remove all lines from [config.localenv] to next non-localenv section or EOF
    # This is a multi-step process to handle complex TOML structure
    
    # Create a temp file without the localenv sections
    awk '
        /^\[config\.localenv/ { skip = 1; next }
        /^\[/ && !/^\[config\.localenv/ { skip = 0 }
        !skip { print }
    ' "${AIKEN_TOML}" > "${AIKEN_TOML}.clean"
    mv "${AIKEN_TOML}.clean" "${AIKEN_TOML}"
fi

# Append [config.localenv] section with dynamic values
echo "Adding [config.localenv] section with local-env one-shot hashes..."
cat >> "${AIKEN_TOML}" << EOF

# Dynamically generated config for local-environment testing
[config.localenv]
cnight_name = "NIGHT"
reserve_one_shot_index = 0
council_one_shot_index = ${COUNCIL_INDEX}
ics_one_shot_index = 0
technical_authority_one_shot_index = ${TECHAUTH_INDEX}
federated_operators_one_shot_index = ${FEDERATEDOPS_INDEX}
main_gov_one_shot_index = 0
staging_gov_one_shot_index = 0
main_council_update_one_shot_index = 0
main_tech_auth_update_one_shot_index = 0
main_federated_ops_update_one_shot_index = 0
committee_bridge_one_shot_index = 0
committee_threshold_one_shot_index = 0
terms_and_conditions_one_shot_index = 0
terms_and_conditions_threshold_one_shot_index = 0
collateral_utxo_index = 0

[config.localenv.reserve_one_shot_hash]
bytes = "0000000000000000000000000000000000000000000000000000000000000000"
encoding = "hex"

[config.localenv.council_one_shot_hash]
bytes = "${COUNCIL_HASH}"
encoding = "hex"

[config.localenv.ics_one_shot_hash]
bytes = "0000000000000000000000000000000000000000000000000000000000000000"
encoding = "hex"

[config.localenv.technical_authority_one_shot_hash]
bytes = "${TECHAUTH_HASH}"
encoding = "hex"

[config.localenv.federated_operators_one_shot_hash]
bytes = "${FEDERATEDOPS_HASH}"
encoding = "hex"

[config.localenv.main_gov_one_shot_hash]
bytes = "0000000000000000000000000000000000000000000000000000000000000000"
encoding = "hex"

[config.localenv.staging_gov_one_shot_hash]
bytes = "0000000000000000000000000000000000000000000000000000000000000000"
encoding = "hex"

[config.localenv.main_council_update_one_shot_hash]
bytes = "0000000000000000000000000000000000000000000000000000000000000000"
encoding = "hex"

[config.localenv.main_tech_auth_update_one_shot_hash]
bytes = "0000000000000000000000000000000000000000000000000000000000000000"
encoding = "hex"

[config.localenv.main_federated_ops_update_one_shot_hash]
bytes = "0000000000000000000000000000000000000000000000000000000000000000"
encoding = "hex"

[config.localenv.committee_bridge_one_shot_hash]
bytes = "0000000000000000000000000000000000000000000000000000000000000000"
encoding = "hex"

[config.localenv.committee_threshold_one_shot_hash]
bytes = "0000000000000000000000000000000000000000000000000000000000000000"
encoding = "hex"

[config.localenv.terms_and_conditions_one_shot_hash]
bytes = "0000000000000000000000000000000000000000000000000000000000000000"
encoding = "hex"

[config.localenv.terms_and_conditions_threshold_one_shot_hash]
bytes = "0000000000000000000000000000000000000000000000000000000000000000"
encoding = "hex"

[config.localenv.collateral_utxo_hash]
bytes = "0000000000000000000000000000000000000000000000000000000000000000"
encoding = "hex"
EOF

echo "✓ Config section added"

# Compile contracts using aiken directly with localenv config
# Note: We don't use build_contracts.sh as it requires toml-cli and does
# multi-stage compilation. For forever contracts, a simple aiken build suffices.
echo "Compiling Aiken contracts with localenv config..."

# aiken build may return non-zero for test failures but still generate plutus.json
aiken build --env localenv || true

# Check if plutus.json was generated
if [[ ! -f "${PLUTUS_JSON}" ]]; then
    echo "ERROR: plutus.json not generated after compilation"
    exit 1
fi

echo "✓ Contracts compiled successfully"

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
