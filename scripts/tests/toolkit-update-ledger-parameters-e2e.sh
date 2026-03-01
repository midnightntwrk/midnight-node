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

set -euxo pipefail

NODE_IMAGE="$1"
TOOLKIT_IMAGE="$2"
RNG_SEED="0000000000000000000000000000000000000000000000000000000000000037"

echo "🎯 Running Toolkit E2E test"
echo "🧱 NODE_IMAGE: $NODE_IMAGE"
echo "🧱 TOOLKIT_IMAGE: $TOOLKIT_IMAGE"

# Ensure Docker network exists
docker network create ledger-params-e2e-net || true

# Start node in background (without --rm so we can get logs on failure)
echo "🚀 Starting node container..."
docker run -d \
  --name midnight-node \
  --network ledger-params-e2e-net \
  -e CFG_PRESET=dev \
  -e SIDECHAIN_BLOCK_BENEFICIARY="04bcf7ad3be7a5c790460be82a713af570f22e0f801f6659ab8e84a52be6969e" \
  "$NODE_IMAGE"

cleanup() {
    echo "🛑 Cleaning up..."
    # Show logs if container exists (helpful for debugging crashes)
    if docker container inspect midnight-node &>/dev/null; then
        echo "📋 Node container logs:"
        docker logs midnight-node --tail 100 || true
    fi
    docker container stop midnight-node || true
    docker container rm midnight-node || true
    docker network rm ledger-params-e2e-net || true
}
# --- Always-cleanup: runs on success, error, or interrupt ---
trap cleanup EXIT

# Wait for node to be ready with health check
echo "⏳ Waiting for node to boot..."
MAX_ATTEMPTS=30
ATTEMPT=0
while [ $ATTEMPT -lt $MAX_ATTEMPTS ]; do
    ATTEMPT=$((ATTEMPT + 1))
    
    # Check if container is still running
    if ! docker container inspect midnight-node --format '{{.State.Running}}' 2>/dev/null | grep -q true; then
        echo "❌ Node container is not running!"
        echo "📋 Container status:"
        docker container inspect midnight-node --format '{{.State.Status}} - Exit code: {{.State.ExitCode}}' || true
        echo "📋 Container logs:"
        docker logs midnight-node || true
        exit 1
    fi
    
    # Try to connect to RPC endpoint
    if docker run --rm --network ledger-params-e2e-net curlimages/curl:latest \
        --silent --fail --max-time 2 \
        -H "Content-Type: application/json" \
        -d '{"jsonrpc":"2.0","method":"system_health","params":[],"id":1}' \
        http://midnight-node:9944 > /dev/null 2>&1; then
        echo "✅ Node is ready after $ATTEMPT attempts"
        break
    fi
    
    echo "⏳ Waiting for node... (attempt $ATTEMPT/$MAX_ATTEMPTS)"
    sleep 2
done

if [ $ATTEMPT -eq $MAX_ATTEMPTS ]; then
    echo "❌ Node failed to become ready within timeout"
    exit 1
fi

# Allow a couple more blocks to be produced
sleep 10

# Run toolkit commands
echo "📦 Running toolkit tests..."

echo "Get version for toolkit"
docker run --rm -e RUST_BACKTRACE=1 --network ledger-params-e2e-net "$TOOLKIT_IMAGE" version

current_parameters=$(
    docker run --rm -e RESTORE_OWNER="$(id -u):$(id -g)" -e RUST_BACKTRACE=1 --network ledger-params-e2e-net "$TOOLKIT_IMAGE" \
        show-ledger-parameters -r ws://midnight-node:9944 --serialize
)

docker run --rm -e RESTORE_OWNER="$(id -u):$(id -g)" -e RUST_BACKTRACE=1 --network ledger-params-e2e-net "$TOOLKIT_IMAGE" \
    update-ledger-parameters -r ws://midnight-node:9944 -t //Alice -t //Bob -c //Dave -c //Eve --c-to-m-bridge-min-amount 2000

new_parameters=$(
    docker run --rm -e RESTORE_OWNER="$(id -u):$(id -g)" -e RUST_BACKTRACE=1 --network ledger-params-e2e-net "$TOOLKIT_IMAGE" \
        show-ledger-parameters -r ws://midnight-node:9944 --serialize
)

if [ "$current_parameters" != "$new_parameters" ]; then
  echo "✅ Ledger parameters update successful"
else
  echo "❌ Ledger parameters update failed"
  exit 1
fi

echo "✅ Toolkit E2E"
