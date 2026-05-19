#!/usr/bin/env bash

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

# Verifies that pallet-batch correctly batches governance calls: an
# `update-ledger-parameters` call and a `runtime-upgrade` (authorize_upgrade) call
# are encoded, batched via `Batch::batch_all`, dispatched through one
# federated-authority motion, and the upgrade is then applied. The runtime WASM
# is extracted from the node image so the upgrade is a no-op same-bytes swap
# that still exercises the full authorize → apply → CodeUpdated path.

set -euxo pipefail

NODE_IMAGE="$1"
TOOLKIT_IMAGE="$2"

NETWORK="batch-gov-e2e-net"
NODE_NAME="midnight-node-batch-gov"
WASM_DIR=$(mktemp -d)
WASM_FILE="$WASM_DIR/runtime.wasm"
chmod 755 "$WASM_DIR"

echo "🎯 Running Batch Governance E2E test"
echo "🧱 NODE_IMAGE: $NODE_IMAGE"
echo "🧱 TOOLKIT_IMAGE: $TOOLKIT_IMAGE"

cleanup() {
    echo "🛑 Cleaning up..."
    if docker container inspect "$NODE_NAME" &>/dev/null; then
        echo "📋 Node container logs:"
        docker logs "$NODE_NAME" --tail 100 || true
    fi
    docker container stop "$NODE_NAME" || true
    docker container rm "$NODE_NAME" || true
    docker network rm "$NETWORK" || true
    rm -rf "$WASM_DIR"
}
trap cleanup EXIT

docker network create "$NETWORK" || true

# Extract runtime WASM from the node image. The image stores it under
# /artifacts-<arch>/, so glob to remain arch-agnostic.
echo "📥 Extracting runtime WASM from node image..."
docker run --rm --entrypoint sh "$NODE_IMAGE" \
    -c 'cat /artifacts-*/midnight_node_runtime.compact.compressed.wasm' \
    > "$WASM_FILE"
chmod 644 "$WASM_FILE"
WASM_BYTES=$(wc -c < "$WASM_FILE")
echo "📥 Wrote $WASM_BYTES bytes to $WASM_FILE"
if [ "$WASM_BYTES" -lt 1000 ]; then
    echo "❌ Runtime WASM looks too small — likely an extraction error"
    exit 1
fi

echo "🚀 Starting node container..."
docker run -d \
    --name "$NODE_NAME" \
    --network "$NETWORK" \
    -e CFG_PRESET=dev \
    -e SIDECHAIN_BLOCK_BENEFICIARY="04bcf7ad3be7a5c790460be82a713af570f22e0f801f6659ab8e84a52be6969e" \
    "$NODE_IMAGE"

echo "⏳ Waiting for node to boot..."
MAX_ATTEMPTS=30
ATTEMPT=0
while [ $ATTEMPT -lt $MAX_ATTEMPTS ]; do
    ATTEMPT=$((ATTEMPT + 1))

    if ! docker container inspect "$NODE_NAME" --format '{{.State.Running}}' 2>/dev/null | grep -q true; then
        echo "❌ Node container is not running!"
        docker container inspect "$NODE_NAME" --format '{{.State.Status}} - Exit code: {{.State.ExitCode}}' || true
        docker logs "$NODE_NAME" || true
        exit 1
    fi

    if docker run --rm --network "$NETWORK" curlimages/curl:latest \
        --silent --fail --max-time 2 \
        -H "Content-Type: application/json" \
        -d '{"jsonrpc":"2.0","method":"system_health","params":[],"id":1}' \
        "http://${NODE_NAME}:9944" >/dev/null 2>&1; then
        echo "✅ Node ready after $ATTEMPT attempts"
        break
    fi

    echo "⏳ Waiting for node... ($ATTEMPT/$MAX_ATTEMPTS)"
    sleep 2
done

if [ $ATTEMPT -eq $MAX_ATTEMPTS ]; then
    echo "❌ Node failed to become ready"
    exit 1
fi

# Allow a couple more blocks to be produced
sleep 10

echo "📦 Toolkit version:"
docker run --rm --network "$NETWORK" "$TOOLKIT_IMAGE" version

# Capture initial ledger params
INITIAL_PARAMS=$(
    docker run --rm --network "$NETWORK" "$TOOLKIT_IMAGE" \
        show-ledger-parameters -r "ws://${NODE_NAME}:9944" --serialize
)

# Step 1: encode update-ledger-parameters with a deliberate change.
echo "🧮 Encoding update-ledger-parameters..."
HEX_LEDGER=$(
    docker run --rm --network "$NETWORK" "$TOOLKIT_IMAGE" --quiet \
        update-ledger-parameters -r "ws://${NODE_NAME}:9944" \
        --c-to-m-bridge-min-amount 4242 --encode-only
)
echo "   → ${#HEX_LEDGER} hex chars"

# Step 2: encode runtime-upgrade against the WASM extracted from the image.
echo "🧮 Encoding runtime-upgrade..."
HEX_UPGRADE=$(
    docker run --rm --network "$NETWORK" -v "$WASM_DIR:/wasm" "$TOOLKIT_IMAGE" --quiet \
        runtime-upgrade -r "ws://${NODE_NAME}:9944" \
        --wasm-file /wasm/runtime.wasm --encode-only
)
echo "   → ${#HEX_UPGRADE} hex chars"

# Step 3: batch + dispatch via federated authority.
echo "📨 Submitting batched governance motion..."
docker run --rm --network "$NETWORK" "$TOOLKIT_IMAGE" \
    batch -r "ws://${NODE_NAME}:9944" \
    --encoded-call "$HEX_LEDGER" --encoded-call "$HEX_UPGRADE" \
    -t //Alice -t //Bob -c //Dave -c //Eve

# Step 4: apply the authorized upgrade.
echo "📨 Applying authorized upgrade..."
docker run --rm --network "$NETWORK" -v "$WASM_DIR:/wasm" "$TOOLKIT_IMAGE" \
    apply-authorized-upgrade -r "ws://${NODE_NAME}:9944" \
    --wasm-file /wasm/runtime.wasm

# Verify ledger params actually changed.
NEW_PARAMS=$(
    docker run --rm --network "$NETWORK" "$TOOLKIT_IMAGE" \
        show-ledger-parameters -r "ws://${NODE_NAME}:9944" --serialize
)

if [ "$INITIAL_PARAMS" != "$NEW_PARAMS" ]; then
    echo "✅ Batched ledger-parameters update applied"
else
    echo "❌ Ledger parameters did not change — batch motion did not apply"
    exit 1
fi

echo "✅ Toolkit Batch Governance E2E"
