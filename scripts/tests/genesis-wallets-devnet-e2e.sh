#!/usr/bin/env bash
set -euxo pipefail

NODE_IMAGE="$1"
GENERATOR_IMAGE="$2"
NETWORK="${3:-midnight-net-genesis-devnet}"
NODE_CONTAINER="${4:-midnight-node-genesis-devnet}"

echo "🎯 Running Genesis Wallets E2E test"
echo "🧱 NODE_IMAGE: $NODE_IMAGE"
echo "🧱 GENERATOR_IMAGE: $GENERATOR_IMAGE"

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
  -e MAIN_CHAIN_FOLLOWER_MOCK_REGISTRATIONS_FILE="/res/mock-bridge-data/qanet-mock.json" \
  "$NODE_IMAGE"

echo "⏳ Waiting for node to boot..."
sleep 30

# Run wallets check script
echo "📦 Running genesis wallets tests..."
GENERATOR_IMAGE="$GENERATOR_IMAGE" NETWORK="$NETWORK" NODE_CONTAINER="$NODE_CONTAINER" bash ./scripts/genesis_wallets_test.sh || TEST_FAILED=true

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
