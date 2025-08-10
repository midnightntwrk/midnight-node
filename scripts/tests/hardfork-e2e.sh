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

NODE_IMAGE=${1:?Usage: $0 <NODE_IMAGE> <UPGRADER_IMAGE>}
UPGRADER_IMAGE=${2:?Usage: $0 <NODE_IMAGE> <UPGRADER_IMAGE>}

echo "🧱 Running hardfork E2E with:"
echo "    NODE_IMAGE=$NODE_IMAGE"
echo "    UPGRADER_IMAGE=$UPGRADER_IMAGE"

docker network create midnight-net-hardfork || true

echo "🚀 Launching node container..."
docker run -d --rm \
  --network midnight-net-hardfork \
  -p 9944:9944 \
  --name midnight-node \
  -e CFG_PRESET=dev \
  -e SIDECHAIN_BLOCK_BENEFICIARY="04bcf7ad3be7a5c790460be82a713af570f22e0f801f6659ab8e84a52be6969e" \
  "$NODE_IMAGE"

echo "⏳ Waiting for node to boot..."
sleep 10

echo "🔍 Fetching initial spec version..."
RPC_PAYLOAD='{"jsonrpc": "2.0", "id": 1, "method": "chain_getHeader", "params": []}'
SPEC_VERSION=$(curl -s -X POST http://localhost:9944 \
  -H "Content-Type: application/json" \
  -d "$RPC_PAYLOAD" | jq -r '.result.digest.logs[] | select(startswith("0x044d4e5356")) | .[14:]')

echo "Initial spec version: $SPEC_VERSION"

echo "⚙️ Running upgrader container..."
docker run --rm --network host "$UPGRADER_IMAGE" -t 0

echo "⏳ Waiting post-upgrade..."
sleep 10

echo "🔍 Fetching new spec version..."
NEW_SPEC_VERSION=$(curl -s -X POST http://localhost:9944 \
  -H "Content-Type: application/json" \
  -d "$RPC_PAYLOAD" | jq -r '.result.digest.logs[] | select(startswith("0x044d4e5356")) | .[14:]')

echo "New spec version: $NEW_SPEC_VERSION"

if [ "$SPEC_VERSION" != "$NEW_SPEC_VERSION" ]; then
  echo "✅ Upgrade successful: $SPEC_VERSION → $NEW_SPEC_VERSION"
  exit 0
else
  echo "❌ Upgrade failed: $SPEC_VERSION → $NEW_SPEC_VERSION"
  exit 1
fi
