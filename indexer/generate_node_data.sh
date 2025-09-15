#!/usr/bin/env bash

set -euxo pipefail

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

# Function to run all toolkit commands
run_toolkit_commands() {
    # Generate batches
    # Note: Reduced from -n 3 -b 2 to -n 1 -b 1 to minimize DUST requirements
    # after fees were enabled in node 0.16.0-da0b6c69. Larger batch sizes fail with:
    # "Balancing TX failed: Insufficient DUST (trying to spend X, need Y more)"
    # This matches the approach used in midnight-node's toolkit-e2e.sh CI tests.
    docker run \
        --rm \
        --network host \
        -v /tmp:/out \
        ghcr.io/midnight-ntwrk/midnight-node-toolkit:$node_version \
        generate-txs batches -n 1 -b 1

    docker run \
        --rm \
        --network host \
        -v /tmp:/out \
        ghcr.io/midnight-ntwrk/midnight-node-toolkit:$node_version \
        generate-txs --dest-file /out/contract_tx_1_deploy.mn --to-bytes \
        contract-calls deploy \
        --rng-seed '0000000000000000000000000000000000000000000000000000000000000037'

    docker run \
        --rm \
        --network host \
        -v /tmp:/out \
        ghcr.io/midnight-ntwrk/midnight-node-toolkit:$node_version \
        contract-address --network undeployed \
        --src-file /out/contract_tx_1_deploy.mn --dest-file /out/contract_address.mn

    docker run \
        --rm \
        --network host \
        -v /tmp:/out \
        ghcr.io/midnight-ntwrk/midnight-node-toolkit:$node_version \
        generate-txs --src-files /out/contract_tx_1_deploy.mn --dest-url ws://127.0.0.1:9944 \
        send

    # The 'store' function inserts data into a Merkle tree in the test contract
    # (see midnight-node MerkleTreeContract). We need this to generate contract
    # action events in the test data so the indexer can verify it properly tracks
    # and indexes contract state changes.
    docker run \
        --rm \
        --network host \
        -v /tmp:/out \
        ghcr.io/midnight-ntwrk/midnight-node-toolkit:$node_version \
        generate-txs contract-calls call \
        --call-key store \
        --rng-seed '0000000000000000000000000000000000000000000000000000000000000037' \
        --contract-address /out/contract_address.mn

    # Wait for the contract call to be finalized before running maintenance.
    sleep 15

    docker run \
        --rm \
        --network host \
        -v /tmp:/out \
        ghcr.io/midnight-ntwrk/midnight-node-toolkit:$node_version \
        generate-txs contract-calls maintenance \
        --rng-seed '0000000000000000000000000000000000000000000000000000000000000037' \
        --contract-address /out/contract_address.mn
}

# Clean up any existing data
if [ -d ./.node/$node_version ]; then
    rm -r ./.node/$node_version;
fi

mkdir -p ./.node/$node_version

# Start the node container
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
    -v ./.node/$node_version:/node \
    ghcr.io/midnight-ntwrk/midnight-node:$node_version

# Wait for node to be ready (max 30 seconds)
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

# Retry the entire toolkit command sequence up to 3 times
max_attempts=3
attempt=1

while [ $attempt -le $max_attempts ]; do
    echo "Running toolkit commands (attempt $attempt of $max_attempts)..."

    # Try to run all toolkit commands
    if run_toolkit_commands; then
        echo "Successfully generated node data"
        exit 0
    fi

    echo "Toolkit commands failed on attempt $attempt" >&2

    # If this wasn't the last attempt, clean up and retry
    if [ $attempt -lt $max_attempts ]; then
        echo "Cleaning up node data folder for retry..." >&2
        rm -rf ./.node/$node_version/*
        echo "Waiting before retry..." >&2
        sleep $((attempt * 5))
    fi

    attempt=$((attempt + 1))
done

echo "Failed to generate node data after $max_attempts attempts" >&2
# Clean up the folder on final failure
rm -rf ./.node/$node_version
exit 1
