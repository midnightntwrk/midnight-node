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

# Script to reproduce the maintenance transaction signing bug
# Bug: Maintenance transactions fail with ThresholdMissed error due to missing signature support
# Issue: generate-txs contract-simple maintenance cannot add required signatures

set -euxo pipefail

NODE_IMAGE="${1:-ghcr.io/midnight-ntwrk/midnight-node:0.18.0-rc.3}"
TOOLKIT_IMAGE="${2:-ghcr.io/midnight-ntwrk/midnight-node-toolkit:0.18.0-rc.3}"

echo "🐛 Reproducing Maintenance Transaction Bug"
echo "📋 NODE_IMAGE: $NODE_IMAGE"
echo "📋 TOOLKIT_IMAGE: $TOOLKIT_IMAGE"
echo ""

# Ensure Docker network exists
docker network create midnight-net-maintenance-bug || true

# Start node in background
echo "🚀 Starting node container..."
docker run -d --rm \
  --name midnight-node-maintenance-bug \
  --network midnight-net-maintenance-bug \
  -p 9945:9944 \
  -e CFG_PRESET=dev \
  -e SIDECHAIN_BLOCK_BENEFICIARY="04bcf7ad3be7a5c790460be82a713af570f22e0f801f6659ab8e84a52be6969e" \
  "$NODE_IMAGE"

tempdir=$(mktemp -d 2>/dev/null || mktemp -d -t 'maintenancebug')
cleanup() {
    echo ""
    echo "🛑 Cleaning up..."
    echo "🛑 Killing node container..."
    docker container stop midnight-node-maintenance-bug 2>/dev/null || true
    echo "🧹 Removing tempdir..."
    rm -rf $tempdir
}
trap cleanup EXIT

echo "⏳ Waiting for node to boot..."
sleep 5

# Run toolkit commands
echo "📦 Setting up test environment..."

deploy_tx_filename="deploy_tx.mn"
maintenance_tx_filename="maintenance_tx.mn"
contract_dir="contract"

# Compile counter contract is included in the toolkit image
# Copy it out to simulate compiling a contract externally
tmpid=$(docker create "$TOOLKIT_IMAGE")
docker cp "$tmpid:/toolkit-js/test/contract" "$tempdir/$contract_dir"
docker rm -v $tmpid

coin_public=$(
    docker run --rm -e RUST_BACKTRACE=1 "$TOOLKIT_IMAGE" \
    show-address \
    --network undeployed \
    --seed 0000000000000000000000000000000000000000000000000000000000000001 \
    --coin-public
)

echo ""
echo "📝 Step 1: Deploy a contract..."
echo "=================================="
echo "Generate deploy intent"
docker run --rm -e RUST_BACKTRACE=1 --network container:midnight-node-maintenance-bug \
    -e RESTORE_OWNER="$(id -u):$(id -g)" \
    -v $tempdir:/out -v $tempdir/$contract_dir:/toolkit-js/contract \
    "$TOOLKIT_IMAGE" \
    generate-intent deploy -c /toolkit-js/contract/contract.config.ts \
    --coin-public "$coin_public" \
    --output-intent "/out/deploy.bin" \
    --output-private-state "/out/initial_state.json" \
    --output-zswap-state "/out/temp.json" \
    20

echo "Generate deploy tx"
docker run --rm -e RUST_BACKTRACE=1 --network container:midnight-node-maintenance-bug \
    -e RESTORE_OWNER="$(id -u):$(id -g)" \
    -v $tempdir:/out -v $tempdir/$contract_dir:/toolkit-js/contract \
    "$TOOLKIT_IMAGE" \
    send-intent \
    --intent-file "/out/deploy.bin" \
    --compiled-contract-dir contract/managed/counter \
    --to-bytes --dest-file "/out/$deploy_tx_filename"

echo "Send deploy tx"
docker run --rm -e RUST_BACKTRACE=1 --network container:midnight-node-maintenance-bug \
    -e RESTORE_OWNER="$(id -u):$(id -g)" \
    -v $tempdir:/out -v $tempdir/$contract_dir:/toolkit-js/contract \
    "$TOOLKIT_IMAGE" \
    generate-txs --src-file /out/$deploy_tx_filename -r 1 send

contract_address=$(
    docker run --rm -e RUST_BACKTRACE=1 --network container:midnight-node-maintenance-bug \
    -e RESTORE_OWNER="$(id -u):$(id -g)" \
    -v $tempdir:/out -v $tempdir/$contract_dir:/toolkit-js/contract \
    "$TOOLKIT_IMAGE" \
    contract-address \
    --src-file /out/$deploy_tx_filename
)

echo ""
echo "✅ Contract deployed successfully!"
echo "📍 Contract Address: $contract_address"
echo ""

echo "📝 Step 2: Attempt to generate maintenance transaction..."
echo "=========================================================="
echo "🐛 This is where the bug occurs - maintenance transaction generation fails"
echo "   because the toolkit cannot add required signatures."
echo ""

echo "Running: generate-txs contract-simple maintenance"
echo "Command (matches toolkit-e2e.sh pattern): generate-txs contract-simple maintenance \\"
echo "  --rng-seed 0000000000000000000000000000000000000000000000000000000000000001 \\"
echo "  --contract-address $contract_address"
echo ""
echo "Note: We're using --dest-file to demonstrate the failure occurs during generation,"
echo "      but the command structure matches toolkit-e2e.sh (which directly sends)."
echo ""

# Attempt to generate maintenance transaction - this should fail
# Note: Using --dest-file to capture the error, but the command structure matches toolkit-e2e.sh
set +e
docker run --rm -e RUST_BACKTRACE=1 --network container:midnight-node-maintenance-bug \
    -e RESTORE_OWNER="$(id -u):$(id -g)" \
    -v $tempdir:/out -v $tempdir/$contract_dir:/toolkit-js/contract \
    "$TOOLKIT_IMAGE" \
    generate-txs contract-simple maintenance \
    --commitee-seed 0000000000000000000000000000000000000000000000000000000000000001 \
    --new-commitee-seed 1000000000000000000000000000000000000000000000000000000000000001 \
    --rng-seed 0000000000000000000000000000000000000000000000000000000000000001 \
    --contract-address "$contract_address" \
    --dest-file "/out/$maintenance_tx_filename" \
    --to-bytes 2>&1 | tee /tmp/maintenance_error.log

exit_code=$?
set -e

echo ""
echo "=========================================================="
if [ $exit_code -ne 0 ]; then
    echo "❌ BUG REPRODUCED: Maintenance transaction generation failed!"
    echo ""
    echo "Expected Error: ThresholdMissed"
    if grep -q "ThresholdMissed" /tmp/maintenance_error.log; then
        echo "✅ Error confirmed: ThresholdMissed error detected"
        echo ""
        echo "Error Details:"
        grep -A 5 "ThresholdMissed" /tmp/maintenance_error.log || cat /tmp/maintenance_error.log
        echo ""
        echo "🐛 Root Cause:"
        echo "   - Maintenance transactions require signatures from the contract's authority committee"
        echo "   - The toolkit does not provide any way to specify signing keys"
        echo "   - Transaction is created with signatures: [] (empty)"
        echo "   - Validation fails because 0 signatures < threshold (1)"
        echo ""
        echo "📋 Issue Details:"
        echo "   - Command: generate-txs contract-simple maintenance"
        echo "   - No --authority-seeds or similar parameter available"
        echo "   - Alternative command 'generate-intent maintain-contract' exists in codebase"
        echo "     but is not registered in CLI and therefore not accessible"
        echo ""
        echo "✅ Bug reproduction successful!"
        exit 1
    else
        echo "⚠️  Unexpected error occurred (not ThresholdMissed)"
        cat /tmp/maintenance_error.log
        exit 1
    fi
else
    echo "⚠️  Unexpected: Maintenance transaction generation succeeded!"
    echo "   This might mean the bug has been fixed or the test environment is different."
    if [ -f "$tempdir/$maintenance_tx_filename" ]; then
        echo "   Maintenance transaction file was created: $tempdir/$maintenance_tx_filename"
    fi
    exit 0
fi

