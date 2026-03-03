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

# Test script to verify the toolkit doesn't hang when using multiple --dest-url options.
#
# Usage:
#   ./toolkit-multi-dest-e2e.sh <toolkit-binary-or-image>                 # Local mode (uses local-environment)
#
# Examples:
#   ./toolkit-multi-dest-e2e.sh ./target/release/midnight-node-toolkit
#   ./toolkit-multi-dest-e2e.sh ghcr.io/midnight-ntwrk/midnight-node-toolkit:latest

set -euxo pipefail

# Detect mode based on arguments
if [[ $# -eq 1 && -x "$1" ]]; then
    # Local mode: single executable = toolkit binary (requires local-environment)
    TOOLKIT_BINARY="$(realpath "$1")"
    TOOLKIT_IMAGE=""
    echo "TOOLKIT_BINARY=$TOOLKIT_BINARY"
elif [[ $# -eq 1 ]]; then
    # CI mode with just toolkit image (for backwards compatibility) - requires local-environment
    TOOLKIT_IMAGE="$1"
    TOOLKIT_BINARY=""
    echo "Docker mode (requires local-environment): TOOLKIT_IMAGE=$TOOLKIT_IMAGE"
else
    echo "Usage: $0 <toolkit-binary-or-image>               # requires local-environment"
    exit 1
fi

# Funded source seeds from genesis
SEED_1="0000000000000000000000000000000000000000000000000000000000000001"
SEED_2="0000000000000000000000000000000000000000000000000000000000000002"
SEED_3="0000000000000000000000000000000000000000000000000000000000000003"

TIMEOUT_SECONDS=300
NUM_WALLETS=3  # One wallet per funding seed to avoid DUST conflicts

# Create temp directory for transaction files
tempdir=$(mktemp -d 2>/dev/null || mktemp -d -t 'multidest')
echo "Using temp directory: $tempdir"

cleanup() {
    echo "Cleaning up..."
    echo "Removing tempdir..."
    if [[ -n "$TOOLKIT_IMAGE" ]] && [[ -d "$tempdir" ]]; then
        # Use Docker to remove files created by container (as root)
        docker run --rm -v "$tempdir:/out" alpine rm -rf /out/* 2>/dev/null || true
    fi
    rm -rf "$tempdir" || true
}
trap cleanup EXIT

NODE_1="ws://localhost:9933"
NODE_2="ws://localhost:9934"
NODE_3="ws://localhost:9935"
NODE_4="ws://localhost:9936"

echo "Checking if local-environment is running..."
if ! ./local-environment/check-health.sh -u http://localhost:9933 -t 30; then
    echo "ERROR: local-environment is not running"
    echo "Please start it with: cd local-environment && npm run run:local-env"
    exit 1
fi

echo "Running Toolkit Multi-Destination URL E2E Test"

# Helper function to run toolkit (binary or Docker)
run_toolkit() {
    if [[ -n "$TOOLKIT_BINARY" ]]; then
        RUST_BACKTRACE=1 "$TOOLKIT_BINARY" "$@"
    else
        docker run --rm --network host \
            -e RUST_BACKTRACE=1 \
            -e RESTORE_OWNER="$(id -u):$(id -g)" \
            -v "$tempdir:/out" \
            "$TOOLKIT_IMAGE" \
            "$@"
    fi
}

# Helper to get the output path (handles Docker vs binary paths)
# In Docker mode, files are at /out; in binary mode, they're at $tempdir
out_path() {
    local filename="$1"
    if [[ -n "$TOOLKIT_BINARY" ]]; then
        echo "$tempdir/$filename"
    else
        echo "/out/$filename"
    fi
}

# Helper to get unshielded address for a seed
get_address() {
    local seed="$1"
    run_toolkit show-address --network undeployed --seed "$seed" --unshielded
}

echo "Step 1: Get toolkit version"
run_toolkit version

echo "Step 2: Set up UTXOs on chain by funding destination wallets"
# Fund wallets in batches of 2 using single-tx (max 2 outputs per tx)
# Use different source seeds for parallel execution

# Get destination addresses for wallets 0x10 to 0x10+NUM_WALLETS
echo "Generating destination addresses..."
DEST_ADDRESSES=()
START_WALLET=16  # 0x10
END_WALLET=$((START_WALLET + NUM_WALLETS - 1))
for i in $(seq $START_WALLET $END_WALLET); do
    seed=$(printf "%064x" "$i")
    addr=$(get_address "$seed")
    DEST_ADDRESSES+=("$addr")
done

echo "Funding ${#DEST_ADDRESSES[@]} destination wallets..."

# Fund in batches of 2 using the 3 source seeds in round-robin
# This creates the UTXOs needed for the test
SOURCE_SEEDS=("$SEED_1" "$SEED_2" "$SEED_3")
for ((i=0; i<${#DEST_ADDRESSES[@]}; i+=2)); do
    source_idx=$((i/2 % 3))
    source_seed="${SOURCE_SEEDS[$source_idx]}"

    addr1="${DEST_ADDRESSES[$i]}"
    addr2="${DEST_ADDRESSES[$((i+1))]:-}"  # May be empty for last iteration

    if [ -n "$addr2" ]; then
        echo "Funding wallets $((i+16)) and $((i+17)) from source $source_idx..."
        run_toolkit generate-txs single-tx \
            --source-seed "$source_seed" \
            --unshielded-amount 1000000 \
            --destination-address "$addr1" \
            --destination-address "$addr2" \
            -s "$NODE_1" \
            -d "$NODE_1"
    else
        echo "Funding wallet $((i+16)) from source $source_idx..."
        run_toolkit generate-txs single-tx \
            --source-seed "$source_seed" \
            --unshielded-amount 1000000 \
            --destination-address "$addr1" \
            -s "$NODE_1" \
            -d "$NODE_1"
    fi
done

echo "Step 3: Pre-generate transactions to .mn files"
# Generate transactions from funded wallets back to seed 0x01
# Use --funding-seed to pay fees from source seeds that have DUST
DEST_ADDR_01=$(get_address "$SEED_1")

for i in $(seq $START_WALLET $END_WALLET); do
    seed=$(printf "%064x" "$i")
    # Use round-robin funding seed for fees
    funding_idx=$(( (i - START_WALLET) % 3 ))
    funding_seed="${SOURCE_SEEDS[$funding_idx]}"
    echo "Generating transaction from wallet $i (fees from source $funding_idx)..."
    run_toolkit generate-txs single-tx \
        --source-seed "$seed" \
        --funding-seed "$funding_seed" \
        --unshielded-amount 100 \
        --destination-address "$DEST_ADDR_01" \
        -s "$NODE_1" \
        --dest-file "$(out_path "tx_${i}.mn")"
done

echo "Generated $(find "$tempdir" -name 'tx_*.mn' 2>/dev/null | wc -l) transaction files"

echo "Step 4: Send transactions with multiple destination URLs (critical test)"
echo "This tests that the toolkit doesn't hang on exit with multiple --dest-url options"
echo "Timeout set to ${TIMEOUT_SECONDS} seconds"

# Build the list of --src-file arguments
SRC_FILES=()
for f in "$tempdir"/tx_*.mn; do
    SRC_FILES+=(--src-file "$(out_path "$(basename "$f")")")
done

# Run the send with timeout - if it hangs, timeout will kill it with exit code 124
# Note: timeout can't run shell functions, so we construct the command directly
if [[ -n "$TOOLKIT_BINARY" ]]; then
    SEND_CMD=("$TOOLKIT_BINARY" generate-txs "${SRC_FILES[@]}" send --rate 2 -d "$NODE_1" -d "$NODE_2" -d "$NODE_3" -d "$NODE_4")
else
    SEND_CMD=(docker run --rm --network host -e RUST_BACKTRACE=1 -v "$tempdir:/out" "$TOOLKIT_IMAGE" generate-txs "${SRC_FILES[@]}" send --rate 2 -d "$NODE_1" -d "$NODE_2" -d "$NODE_3" -d "$NODE_4")
fi

if RUST_BACKTRACE=1 timeout "$TIMEOUT_SECONDS" "${SEND_CMD[@]}"; then
    echo "SUCCESS: Transactions sent and toolkit exited cleanly (no hang)"
else
    exit_code=$?
    if [ $exit_code -eq 124 ]; then
        echo "FAILED: Toolkit timed out after $TIMEOUT_SECONDS seconds (hang detected)"
        exit 1
    else
        echo "FAILED: Toolkit exited with error code $exit_code"
        exit $exit_code
    fi
fi

echo "Step 5: Verify transactions were processed"
sleep 10
./local-environment/check-health.sh -u http://localhost:9933 -b 50 -t 120

echo "Toolkit Multi-Destination URL E2E Test completed successfully"
