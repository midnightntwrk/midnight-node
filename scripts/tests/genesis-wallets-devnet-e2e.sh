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
NETWORK="${3:-midnight-net-genesis-devnet}"
NODE_CONTAINER="${4:-midnight-node-genesis-devnet}"

echo "🎯 Running Genesis Wallets E2E test"
echo "🧱 NODE_IMAGE: $NODE_IMAGE"
echo "🧱 TOOLKIT_IMAGE: $TOOLKIT_IMAGE"

# Ensure Docker network exists
docker network create $NETWORK || true

# Start node in background
echo "🚀 Starting node container..."
docker run -d --rm \
  --name $NODE_CONTAINER \
  --network $NETWORK \
  -p 9944:9944 \
  -e CFG_PRESET=qanet \
  -e USE_MAIN_CHAIN_FOLLOWER_MOCK=true \
  -e MOCK_REGISTRATIONS_FILE="/res/mock-bridge-data/qanet-mock.json" \
  "$NODE_IMAGE"

echo "⏳ Waiting for node to boot..."
sleep 30

# Run wallets check script
echo "📦 Running genesis wallets tests..."
TOOLKIT_IMAGE="$TOOLKIT_IMAGE" NETWORK="$NETWORK" NODE_CONTAINER="$NODE_CONTAINER" bash ./scripts/genesis_wallets_test.sh || TEST_FAILED=true

# Teardown node
echo "🛑 Cleaning up..."
docker kill $NODE_CONTAINER || true

# Exit with test result
if [ "${TEST_FAILED:-false}" = true ]; then
  echo "❌ Genesis Wallet Tests failed."
  exit 1
else
  echo "✅ Genesis Wallet Tests complete."
fi
