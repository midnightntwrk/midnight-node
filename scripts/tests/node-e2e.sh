#!/usr/bin/env bash
set -euxo pipefail

NODE_IMAGE="$1"

if [ -z "$NODE_IMAGE" ]; then
  echo "❌ Missing required argument: NODE_IMAGE"
  echo "Usage: ./node-e2e-test.sh ghcr.io/midnight-ntwrk/midnight-node:<tag>"
  exit 1
fi

echo "🧪 Running Node E2E tests with:"
echo "    NODE_IMAGE=${NODE_IMAGE}"

# Setup working directory
WORKDIR=$(mktemp -d)
cp -r res ui "$WORKDIR"
cd "$WORKDIR/ui/tests"

# Install dependencies
yarn config set -H enableImmutableInstalls false
yarn install

# Create Docker network
docker network create midnight-net-node || true

# Run the node container
echo "🚀 Launching node container..."
docker run -d --rm \
  --name midnight-node-e2e \
  --network midnight-net-node \
  -p 9944:9944 \
  -e CFG_PRESET=dev \
  -e SIDECHAIN_BLOCK_BENEFICIARY="04bcf7ad3be7a5c790460be82a713af570f22e0f801f6659ab8e84a52be6969e" \
  "${NODE_IMAGE}"

echo "⏳ Waiting for node to start..."
sleep 15

# Run tests
echo "🎯 Running Playwright + Testcontainers tests..."
NODE_IMAGE=$NODE_IMAGE DEBUG='testcontainers*' yarn test:node || TEST_FAILED=true

# Save results
RESULT_DIR="../../../test-artifacts/e2e"
mkdir -p "$RESULT_DIR"
cp -r ./reports/testResults_*.xml "$RESULT_DIR" || true

# Teardown node
echo "🛑 Cleaning up..."
docker kill midnight-node-e2e || true

# Exit with test result
if [ "${TEST_FAILED:-false}" = true ]; then
  echo "❌ Tests failed"
  exit 1
else
  echo "✅ Node E2E tests complete."
fi
