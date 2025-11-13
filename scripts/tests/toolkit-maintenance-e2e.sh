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

set -euxo pipefail

NODE_IMAGE="${1:-ghcr.io/midnight-ntwrk/midnight-node:0.18.0-rc.3}"
TOOLKIT_IMAGE="${2:-ghcr.io/midnight-ntwrk/midnight-node-toolkit:0.18.0-rc.3}"

echo "📋 NODE_IMAGE: $NODE_IMAGE"
echo "📋 TOOLKIT_IMAGE: $TOOLKIT_IMAGE"

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

echo "Deploying counter contract with Toolkit-JS..."

docker run --rm -e RUST_BACKTRACE=1 --network container:midnight-node-maintenance-bug \
    -e RESTORE_OWNER="$(id -u):$(id -g)" \
    -v $tempdir:/out -v $tempdir/$contract_dir:/toolkit-js/contract \
    "$TOOLKIT_IMAGE" \
    generate-intent deploy -c /toolkit-js/contract/contract.config.ts \
    --coin-public "$coin_public" \
    --authority-seed 0000000000000000000000000000000000000000000000000000000000000001 \
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

echo "Switch to use new maintenance authority"
docker run --rm -e RUST_BACKTRACE=1 --network container:midnight-node-maintenance-bug \
    -e RESTORE_OWNER="$(id -u):$(id -g)" \
    -v $tempdir:/out -v $tempdir/$contract_dir:/toolkit-js/contract \
    "$TOOLKIT_IMAGE" \
    generate-txs contract-simple maintenance \
    --authority-seed 0000000000000000000000000000000000000000000000000000000000000001 \
    --new-authority-seed 1000000000000000000000000000000000000000000000000000000000000001 \
    --rng-seed 0000000000000000000000000000000000000000000000000000000000000001 \
    --contract-address "$contract_address"

cp $tempdir/$contract_dir/managed/counter/keys/increment.verifier $tempdir/$contract_dir/managed/counter/keys/increment2.verifier

echo "Add a new increment endpoint, update increment entypoint, and remove the decrement entrypoint"
docker run --rm -e RUST_BACKTRACE=1 --network container:midnight-node-maintenance-bug \
    -e RESTORE_OWNER="$(id -u):$(id -g)" \
    -v $tempdir:/out -v $tempdir/$contract_dir:/toolkit-js/contract \
    "$TOOLKIT_IMAGE" \
    generate-txs contract-simple maintenance \
    --remove-entrypoint decrement \
    --upsert-entrypoint /toolkit-js/contract/managed/counter/keys/increment.verifier \
    --upsert-entrypoint /toolkit-js/contract/managed/counter/keys/increment2.verifier \
    --authority-seed 1000000000000000000000000000000000000000000000000000000000000001 \
    --rng-seed 0000000000000000000000000000000000000000000000000000000000000001 \
    --contract-address "$contract_address"

echo "✅ Toolkit Maintenance"
