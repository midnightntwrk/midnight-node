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
UPGRADER_IMAGE="$2"

echo "🧪 Running Ledger Rollback E2E with:"
echo "    NODE_IMAGE=${NODE_IMAGE}"
echo "    UPGRADER_IMAGE=${UPGRADER_IMAGE}"

docker network create midnight-net-rollback || true

echo "🚀 Launching node container..."
docker run -d --rm \
  --network midnight-net-rollback \
  -p 9944:9944 \
  --name midnight-node \
  -e CFG_PRESET=dev \
  -e SIDECHAIN_BLOCK_BENEFICIARY="04bcf7ad3be7a5c790460be82a713af570f22e0f801f6659ab8e84a52be6969e" \
  "${NODE_IMAGE}"

echo "⏳ Waiting for node to boot..."
sleep 10

echo "🔍 Fetching initial ledger version..."
RPC_PAYLOAD='{"jsonrpc": "2.0", "id": 1, "method": "midnight_ledgerVersion", "params": []}'
INITIAL_LEDGER_VERSION=$(curl -s -X POST http://localhost:9944 \
  -H "Content-Type: application/json" \
  -d "$RPC_PAYLOAD" | jq -r '.result')
echo "Initial ledger version: $INITIAL_LEDGER_VERSION"

echo "⚙️ Running upgrade..."
docker run --rm --network host "${UPGRADER_IMAGE}" -t 0

echo "⏳ Waiting post-upgrade..."
sleep 10

echo "🔍 Fetching new ledger version..."
NEW_LEDGER_VERSION=$(curl -s -X POST http://localhost:9944 \
  -H "Content-Type: application/json" \
  -d "$RPC_PAYLOAD" | jq -r '.result')
echo "New ledger version: $NEW_LEDGER_VERSION"

if [ "$INITIAL_LEDGER_VERSION" != "$NEW_LEDGER_VERSION" ]; then
  echo "✅ Upgrade successful: $INITIAL_LEDGER_VERSION → $NEW_LEDGER_VERSION"
else
  echo "❌ Upgrade failed: $INITIAL_LEDGER_VERSION → $NEW_LEDGER_VERSION"
  exit 1
fi

echo "🔁 Rolling back runtime..."
docker run --rm --network host \
  -e RUNTIME_PATH=/midnight_node_runtime_rollback.compact.compressed.wasm \
  "${UPGRADER_IMAGE}" -t 0

echo "⏳ Waiting post-rollback..."
sleep 10

echo "🔍 Fetching rollback ledger version..."
ROLLED_BACK_LEDGER_VERSION=$(curl -s -X POST http://localhost:9944 \
  -H "Content-Type: application/json" \
  -d "$RPC_PAYLOAD" | jq -r '.result')
echo "Rolled-back ledger version: $ROLLED_BACK_LEDGER_VERSION"

if [ "$INITIAL_LEDGER_VERSION" == "$ROLLED_BACK_LEDGER_VERSION" ]; then
  echo "✅ Rollback successful: $NEW_LEDGER_VERSION → $ROLLED_BACK_LEDGER_VERSION"
  exit 0
else
  echo "❌ Rollback failed: $NEW_LEDGER_VERSION → $ROLLED_BACK_LEDGER_VERSION"
  exit 1
fi
