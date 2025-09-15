#!/usr/bin/env bash

set -euo pipefail

# Cleanup function to ensure node container is removed
cleanup() {
    docker rm -f node >/dev/null 2>&1 || true
}

# Set up trap to cleanup on exit
trap cleanup EXIT

if [ -z "$1" ]; then
    echo "Error: node version parameter is required" >&2
    echo "Usage: $0 <node_version>" >&2
    exit 1
fi
node_version="$1"

# Check if subxt is installed and verify version
if ! command -v subxt &> /dev/null; then
    echo "Error: subxt is not installed. Install it with: cargo install subxt-cli" >&2
    exit 1
fi

# Extract expected version from Cargo.toml
expected_version=$(grep 'subxt.*version' Cargo.toml | sed -E 's/.*version = "([^"]+)".*/\1/')
# Get installed version (subxt outputs version like "subxt-cli 0.44.0-unknown")
installed_version=$(subxt version | sed -E 's/subxt-cli ([0-9]+\.[0-9]+).*/\1/')

if [ "$installed_version" != "$expected_version" ]; then
    echo "Error: subxt version mismatch" >&2
    echo "  Expected: $expected_version (from Cargo.toml)" >&2
    echo "  Installed: $installed_version" >&2
    echo "  Install correct version with: cargo install subxt-cli --version ${expected_version}" >&2
    exit 1
fi

mkdir -p ./.node/$node_version

# SIDECHAIN_BLOCK_BENEFICIARY specifies the wallet that receives block rewards and transaction fees (DUST).
# Required after fees were enabled in 0.16.0-da0b6c69.
# This hex value is a public key that matches the one used in toolkit-e2e.sh.
docker run \
    -d \
    --name node \
    -p 9944:9944 \
    -e SHOW_CONFIG=false \
    -e CFG_PRESET=dev \
    -e SIDECHAIN_BLOCK_BENEFICIARY="04bcf7ad3be7a5c790460be82a713af570f22e0f801f6659ab8e84a52be6969e" \
    ghcr.io/midnight-ntwrk/midnight-node:$node_version

# Wait for port to be available (max 30 seconds)
echo "Waiting for node to be ready..."
for i in {1..30}; do
    if curl -f http://localhost:9944/health/readiness 2>/dev/null; then
        echo "Node is ready"
        sleep 2  # Give it a moment to fully initialize
        break
    fi
    if [ $i -eq 30 ]; then
        echo "Error: Node failed to start after 30 seconds" >&2
        docker logs node 2>&1 | tail -20
        exit 1
    fi
    sleep 1
done

subxt metadata \
    -f bytes \
    --url ws://localhost:9944 > \
    ./.node/$node_version/metadata.scale
