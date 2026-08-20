#!/bin/bash
# Test indexer behaviour during a runtime upgrade (e.g. 0.21 → 0.22)
#
# Uses the same approach as the node CI hardfork test:
# - Generate chain-spec from the old node (embeds old runtime)
# - Start the new node binary with the old chain-spec
# - Perform runtime upgrade via governance using the node-toolkit
#
# Usage:
#   FROM_NODE_TAG=0.21.0 TO_NODE_TAG=0.22.2 INDEXER_TAG=4.1.0-a586cda5 bash qa/scripts/test-runtime-upgrade.sh

set -euo pipefail

FROM_NODE_TAG="${FROM_NODE_TAG:?FROM_NODE_TAG is required (e.g. 0.21.0)}"
TO_NODE_TAG="${TO_NODE_TAG:?TO_NODE_TAG is required (e.g. 0.22.2)}"
INDEXER_TAG="${INDEXER_TAG:?INDEXER_TAG is required}"
NODE_TOOLKIT_TAG="${NODE_TOOLKIT_TAG:-latest-main}"
export IMAGE_REGISTRY="${IMAGE_REGISTRY:-midnightntwrk}"

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
TMPDIR="${REPO_ROOT}/target/runtime-upgrade-test"

echo "=== Runtime Upgrade Test: ${FROM_NODE_TAG} → ${TO_NODE_TAG} ==="
echo "Indexer: ${INDEXER_TAG}"
echo "Toolkit: ${NODE_TOOLKIT_TAG}"

mkdir -p "$TMPDIR"

# --- Step 1: Generate chain-spec from the old node ---
echo ""
echo ">>> Step 1: Generating chain-spec from node ${FROM_NODE_TAG}..."
docker run --rm -e CFG_PRESET=dev "midnightntwrk/midnight-node:${FROM_NODE_TAG}" build-spec > "${TMPDIR}/chainspec.json"
echo "Chain-spec saved ($(wc -c < "${TMPDIR}/chainspec.json") bytes)"

# --- Step 2: Extract new runtime WASM ---
echo ""
echo ">>> Step 2: Extracting runtime WASM from node ${TO_NODE_TAG}..."
ARCH=$(uname -m)
if [ "$ARCH" = "arm64" ] || [ "$ARCH" = "aarch64" ]; then
  WASM_PATH="/artifacts-arm64/midnight_node_runtime.compact.compressed.wasm"
else
  WASM_PATH="/artifacts-amd64/midnight_node_runtime.compact.compressed.wasm"
fi
docker run --rm --entrypoint cat "midnightntwrk/midnight-node:${TO_NODE_TAG}" "$WASM_PATH" > "${TMPDIR}/runtime.wasm"
echo "Runtime WASM saved ($(wc -c < "${TMPDIR}/runtime.wasm") bytes)"

# --- Step 3: Clean up and start environment ---
echo ""
echo ">>> Step 3: Starting local environment (node ${TO_NODE_TAG} with ${FROM_NODE_TAG} chain-spec)..."

# Set NODE_TAG to the new version — the binary that will run
export NODE_TAG="${TO_NODE_TAG}"
export CHAINSPEC_PATH="${TMPDIR}/chainspec.json"

cd "$REPO_ROOT"

# Run the standard cleanup/startup, but use the compose override to mount the chain-spec
# We inline the startup-localenv-from-genesis.sh cleanup logic here to control compose args

# Clean containers
if [ -n "$(docker ps -a | grep -e ghcr.io -e nats -e postgres | awk -F " " '{print $1}')" ]; then
    docker rm -f $(docker ps -a | grep -e ghcr.io -e nats -e postgres | awk -F " " '{print $1}')
fi

# Derive project name
PROJECT_DIR=$(basename "$(pwd)")
DOCKER_PROJECT_NAME=$(echo "$PROJECT_DIR" | tr '[:upper:]' '[:lower:]' | sed 's/\.//g')
DOCKER_VOLUME_NAME="${DOCKER_PROJECT_NAME}_node_data"

# Clean volumes
docker volume rm "$DOCKER_VOLUME_NAME" 2>/dev/null || true

# Clean data dirs
if [ -d "target/data/postgres" ] || [ -d "target/data/nats" ]; then
    docker run --rm -v "$(pwd):/project" alpine sh -c "rm -rf /project/target/data/postgres /project/target/data/nats"
fi
mkdir -p target/data/postgres target/data/nats target/debug

# Create node data volume
docker volume create "$DOCKER_VOLUME_NAME"

# Start with the runtime-upgrade override that mounts the chain-spec
docker compose -f docker-compose.yaml -f docker-compose.runtime-upgrade.yaml --profile cloud up -d

echo "Waiting for indexer API to become ready..."
for i in {1..30}; do
  if curl -sf http://localhost:8088/ready >/dev/null; then
    echo "Indexer API is ready"
    break
  fi
  echo "Not ready yet... ($i)"
  sleep 2
done

# Verify runtime version
echo ""
echo "Verifying runtime version..."
for i in {1..10}; do
  SPEC_VERSION=$(curl -sf -H "Content-Type: application/json" \
    -d '{"id":1,"jsonrpc":"2.0","method":"state_getRuntimeVersion"}' \
    http://localhost:9944 2>/dev/null | python3 -c "import json,sys; print(json.load(sys.stdin)['result']['specVersion'])" 2>/dev/null || echo "")
  if [ -n "$SPEC_VERSION" ]; then
    echo "Current specVersion: ${SPEC_VERSION}"
    break
  fi
  echo "Waiting for node RPC... ($i)"
  sleep 3
done

docker ps --format "table {{.Image}}\t{{.Names}}\t{{.Status}}"

# --- Step 4: Pre-upgrade tests ---
echo ""
echo ">>> Step 4: Run pre-upgrade indexer tests now."
echo "    Press Enter when ready to proceed with the runtime upgrade..."
read -r

# --- Step 5: Perform runtime upgrade via node-toolkit ---
echo ""
echo ">>> Step 5: Performing runtime upgrade via node-toolkit..."

# Determine the docker network name for the toolkit container
NETWORK_NAME="${DOCKER_PROJECT_NAME}_default"

docker run --rm \
  --network "${NETWORK_NAME}" \
  -v "${TMPDIR}/runtime.wasm:/wasm/runtime.wasm" \
  "midnightntwrk/midnight-node-toolkit:${NODE_TOOLKIT_TAG}" \
  runtime-upgrade \
  --wasm-file /wasm/runtime.wasm \
  --rpc-url ws://node:9944 \
  -c "//Eve" \
  -c "//Ferdie" \
  -c "//Dave" \
  -t "//Alice" \
  -t "//Bob" \
  -t "//Charlie" \
  --signer-key "//Alice"

# --- Step 6: Verify runtime upgraded ---
echo ""
echo ">>> Step 6: Verifying runtime upgrade..."
sleep 6
for i in {1..10}; do
  NEW_SPEC_VERSION=$(curl -sf -H "Content-Type: application/json" \
    -d '{"id":1,"jsonrpc":"2.0","method":"state_getRuntimeVersion"}' \
    http://localhost:9944 | python3 -c "import json,sys; print(json.load(sys.stdin)['result']['specVersion'])" 2>/dev/null || echo "")
  if [ -n "$NEW_SPEC_VERSION" ] && [ "$NEW_SPEC_VERSION" != "$SPEC_VERSION" ]; then
    echo "Runtime upgraded: specVersion ${SPEC_VERSION} → ${NEW_SPEC_VERSION}"
    break
  fi
  echo "Waiting for runtime upgrade to take effect... ($i)"
  sleep 6
done

if [ "${NEW_SPEC_VERSION:-}" = "$SPEC_VERSION" ] || [ -z "${NEW_SPEC_VERSION:-}" ]; then
  echo "ERROR: Runtime upgrade did not take effect. specVersion still ${SPEC_VERSION}"
  exit 1
fi

# --- Step 7: Post-upgrade tests ---
echo ""
echo ">>> Step 7: Runtime upgrade complete. Run post-upgrade indexer tests now."
echo "    specVersion: ${SPEC_VERSION} → ${NEW_SPEC_VERSION}"
echo ""
echo "Check indexer logs:"
echo "  docker compose logs chain-indexer --tail 20"
