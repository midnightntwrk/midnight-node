#!/usr/bin/env bash
set -euxo pipefail

NODE_IMAGE="$1"

if [ -z "$NODE_IMAGE" ]; then
  echo "❌ Missing required argument: NODE_IMAGE"
  echo "Usage: ./startup-dev-e2e.sh ghcr.io/midnight-ntwrk/midnight-node:<tag>"
  exit 1
fi

echo "🧪 Running Startup E2E test with:"
echo "    NODE_IMAGE=${NODE_IMAGE}"

# Setup working directory
WORKDIR=$(mktemp -d)
cp -r res "$WORKDIR"

# Create Docker network
docker network create midnight-net-startup || true

# Run the node container
echo "🚀 Launching node container..."
docker run -d --rm \
  --name midnight-node-e2e \
  --network midnight-net-startup \
  -p 9944:9944 \
  -e CFG_PRESET=dev \
  -e SIDECHAIN_BLOCK_BENEFICIARY="04bcf7ad3be7a5c790460be82a713af570f22e0f801f6659ab8e84a52be6969e" \
  "${NODE_IMAGE}"

echo "⏳ Waiting for node to start..."
sleep 30

# ensure node with CFG_PRESET=dev can start fine
(docker logs $(docker ps -q --filter ancestor=${NODE_IMAGE}) 2>&1 | grep "Prepared block for proposing at" && \
docker logs $(docker ps -q --filter ancestor=${NODE_IMAGE}) 2>&1 | grep "finalized #1")
if [ $? -ne 0 ]; then
    echo "❌ Node failed to start with CFG_PRESET=dev"
    TEST_FAILED=true
else
    echo "✅ Node started successfully with CFG_PRESET=dev"
fi

# Teardown node
echo "🛑 Cleaning up..."
docker kill midnight-node-e2e || true

# Exit with test result
if [ "${TEST_FAILED:-false}" = true ]; then
  echo "❌ Startup Test failed."
  exit 1
else
  echo "✅ Startup Test complete."
fi
