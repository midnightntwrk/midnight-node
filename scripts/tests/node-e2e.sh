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

if [ -z "$NODE_IMAGE" ]; then
  echo "❌ Missing required argument: NODE_IMAGE"
  echo "Usage: ./node-e2e.sh ghcr.io/midnight-ntwrk/midnight-node:<tag> ghcr.io/midnight-ntwrk/midnight-node-toolkit:<tag>"
  exit 1
fi

if [ -z "$TOOLKIT_IMAGE" ]; then
  echo "❌ Missing required argument: TOOLKIT_IMAGE"
  echo "Usage: ./node-e2e.sh ghcr.io/midnight-ntwrk/midnight-node:<tag> ghcr.io/midnight-ntwrk/midnight-node-toolkit:<tag>"
  exit 1
fi

echo "🧪 Running Node E2E tests with:"
echo "    NODE_IMAGE=${NODE_IMAGE}"
echo "    TOOLKIT_IMAGE=${TOOLKIT_IMAGE}"

# Setup working directory
WORKDIR=$(mktemp -d)
cp -r res ui "$WORKDIR"
cd "$WORKDIR/ui/tests"

# Install dependencies
yarn config set -H enableImmutableInstalls false
yarn install

# Create Docker network
docker network create midnight-net-node || true

# Override txs files without its context and save the first tx block context timestamp
# This should always work as long as there is less than 3600secs (1h) difference between
# `contract_tx_1_deploy_timestamp_undeployed.txt` creation and the rest of tcs
echo "⚙️ Generating txs to be sent eventually to the chain..."

OUTPUT_DIR="$(realpath ../../res/test-contract)"

docker run --rm \
    -v "$OUTPUT_DIR":/mnt/output \
    -w /mnt/output \
    $TOOLKIT_IMAGE \
    get-tx-from-context \
        --src-file contract_tx_1_deploy_undeployed.mn \
        --dest-file contract_tx_1_deploy_undeployed.mn \
        --network undeployed \
        --from-bytes \
    > "$OUTPUT_DIR/contract_tx_1_deploy_timestamp_undeployed.txt"

docker run --rm \
    -v "$OUTPUT_DIR":/mnt/output \
    -w /mnt/output \
    $TOOLKIT_IMAGE \
    get-tx-from-context \
        --src-file contract_tx_2_store_undeployed.mn \
        --dest-file contract_tx_2_store_undeployed.mn \
        --network undeployed \
        --from-bytes 

docker run --rm \
    -v "$OUTPUT_DIR":/mnt/output \
    -w /mnt/output \
    $TOOLKIT_IMAGE \
    get-tx-from-context \
        --src-file contract_tx_3_check_undeployed.mn \
        --dest-file contract_tx_3_check_undeployed.mn \
        --network undeployed \
        --from-bytes

docker run --rm \
    -v "$OUTPUT_DIR":/mnt/output \
    -w /mnt/output \
    $TOOLKIT_IMAGE \
    get-tx-from-context \
        --src-file contract_tx_4_change_authority_undeployed.mn \
        --dest-file contract_tx_4_change_authority_undeployed.mn \
        --network undeployed \
        --from-bytes

# Run the node container
echo "🚀 Launching node container..."

docker run -d --rm \
  --name midnight-node-e2e \
  --network midnight-net-node \
  --ipc=private \
  -v "$(pwd)/entrypoint.sh:/tmp/entrypoint.sh" \
  -p 9944:9944 \
  -e EPOCH_TIME="$(( $(cat "$OUTPUT_DIR/contract_tx_1_deploy_timestamp_undeployed.txt") + 60 ))" \
  --entrypoint /tmp/entrypoint.sh \
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
