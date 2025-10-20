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

NODE_IMAGE="$1"
TOOLKIT_IMAGE="$2"

echo "🎯 Running Toolkit Contracts E2E test"
echo "🧱 NODE_IMAGE: $NODE_IMAGE"
echo "🧱 TOOLKIT_IMAGE: $TOOLKIT_IMAGE"

# Ensure Docker network exists
docker network create midnight-net-contracts || true

# Start node in background
echo "🚀 Starting node container..."
docker run -d --rm \
  --name midnight-node-contracts \
  --network midnight-net-contracts \
  -p 9944:9944 \
  -e CFG_PRESET=dev \
  -e SIDECHAIN_BLOCK_BENEFICIARY="04bcf7ad3be7a5c790460be82a713af570f22e0f801f6659ab8e84a52be6969e" \
  "$NODE_IMAGE"


tempdir=$(mktemp -d 2>/dev/null || mktemp -d -t 'toolkitcontracts')
cleanup() {
    echo "🛑 Killing node container..."
    docker container stop midnight-node-contracts
    echo "🧹 Removing tempdir..."
    rm -rf $tempdir
}
# --- Always-cleanup: runs on success, error, or interrupt ---
trap cleanup EXIT

echo "⏳ Waiting for node to boot..."
sleep 5

# Run toolkit commands
echo "📦 Running toolkit contract tests..."

deploy_intent_filename="deploy.bin"
deploy_tx_filename="deploy_tx.mn"

address_filename="contract_address.mn"
state_filename="contract_state.mn"

initial_private_state_filename="initial_state.json"
incremented_private_state_filename="increment_state.json"

increment_intent_filename="increment.bin"
increment_tx_filename="increment_tx.mn"

reset_private_state_filename="reset_state.json"
reset_intent_filename="reset.bin"
reset_tx_filename="reset_tx.mn"

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

echo "Generate deploy intent"
docker run --rm -e RUST_BACKTRACE=1 --network container:midnight-node-contracts \
    -e RESTORE_OWNER="$(id -u):$(id -g)" \
    -v $tempdir:/out -v $tempdir/$contract_dir:/toolkit-js/contract \
    "$TOOLKIT_IMAGE" \
    generate-intent deploy -c /toolkit-js/contract/contract.config.ts \
    --coin-public "$coin_public" \
    --output-intent "/out/$deploy_intent_filename" \
    --output-private-state "/out/$initial_private_state_filename" \
    --output-zswap-state "/out/temp.json" \
    20

test -f "$tempdir/$deploy_intent_filename"
test -f "$tempdir/$initial_private_state_filename"

cat "$tempdir/$initial_private_state_filename"

echo "Generate deploy tx"
docker run --rm -e RUST_BACKTRACE=1 --network container:midnight-node-contracts \
    -e RESTORE_OWNER="$(id -u):$(id -g)" \
    -v $tempdir:/out -v $tempdir/$contract_dir:/toolkit-js/contract \
    "$TOOLKIT_IMAGE" \
    send-intent \
    --intent-file "/out/$deploy_intent_filename" \
    --compiled-contract-dir contract/managed/counter \
    --to-bytes --dest-file "/out/$deploy_tx_filename"

test -f "$tempdir/$deploy_tx_filename"

echo "Send deploy tx"
docker run --rm -e RUST_BACKTRACE=1 --network container:midnight-node-contracts \
    -e RESTORE_OWNER="$(id -u):$(id -g)" \
    -v $tempdir:/out -v $tempdir/$contract_dir:/toolkit-js/contract \
    "$TOOLKIT_IMAGE" \
    generate-txs --src-file /out/$deploy_tx_filename -r 1 send

contract_address=$(
    docker run --rm -e RUST_BACKTRACE=1 --network container:midnight-node-contracts \
    -e RESTORE_OWNER="$(id -u):$(id -g)" \
    -v $tempdir:/out -v $tempdir/$contract_dir:/toolkit-js/contract \
    "$TOOLKIT_IMAGE" \
    contract-address \
    --src-file /out/$deploy_tx_filename
)

echo "Get contract state"
docker run --rm -e RUST_BACKTRACE=1 --network container:midnight-node-contracts \
    -e RESTORE_OWNER="$(id -u):$(id -g)" \
    -v $tempdir:/out -v $tempdir/$contract_dir:/toolkit-js/contract \
    "$TOOLKIT_IMAGE" \
    contract-state \
    --contract-address $contract_address \
    --dest-file /out/$state_filename

test -f "$tempdir/$state_filename"

echo "Generate circuit call intent"
docker run --rm -e RUST_BACKTRACE=1 --network container:midnight-node-contracts \
    -e RESTORE_OWNER="$(id -u):$(id -g)" \
    -v $tempdir:/out -v $tempdir/$contract_dir:/toolkit-js/contract \
    "$TOOLKIT_IMAGE" \
    generate-intent circuit -c /toolkit-js/contract/contract.config.ts \
    --coin-public "$coin_public" \
    --input-onchain-state "/out/$state_filename" \
    --input-private-state "/out/$initial_private_state_filename" \
    --contract-address $contract_address \
    --output-intent "/out/$increment_intent_filename" \
    --output-private-state "/out/$incremented_private_state_filename" \
    --output-zswap-state "/out/temp.json" \
    increment

test -f "$tempdir/$increment_intent_filename"
test -f "$tempdir/$incremented_private_state_filename"

echo "Generate circuit call tx"
docker run --rm -e RUST_BACKTRACE=1 --network container:midnight-node-contracts \
    -e RESTORE_OWNER="$(id -u):$(id -g)" \
    -v $tempdir:/out -v $tempdir/$contract_dir:/toolkit-js/contract \
    "$TOOLKIT_IMAGE" \
    send-intent \
    --intent-file "/out/$increment_intent_filename" \
    --compiled-contract-dir /toolkit-js/contract/managed/counter

echo "Generate circuit call intent reset"
docker run --rm -e RUST_BACKTRACE=1 --network container:midnight-node-contracts \
    -e RESTORE_OWNER="$(id -u):$(id -g)" \
    -v $tempdir:/out -v $tempdir/$contract_dir:/toolkit-js/contract \
    "$TOOLKIT_IMAGE" \
    generate-intent circuit -c /toolkit-js/contract/contract.config.ts \
    --coin-public "$coin_public" \
    --input-onchain-state "/out/$state_filename" \
    --input-private-state "/out/$incremented_private_state_filename" \
    --contract-address $contract_address \
    --output-intent "/out/$reset_intent_filename" \
    --output-private-state "/out/$reset_private_state_filename" \
    --output-zswap-state "temp.json" \
    reset

# After "Generate circuit call intent reset" the private state must be {"count":0}
set +x
actual_state=$(cat "$tempdir/$reset_private_state_filename")
echo "📄 Reset private state (expected: {\"count\":0}, actual: $actual_state)"
if [ "$actual_state" != '{"count":0}' ]; then
  echo "❌ Error: reset_private_state.json content is not {\"count\":0}"
  exit 1
fi
set -x

echo "✅ Toolkit Contracts E2E"
