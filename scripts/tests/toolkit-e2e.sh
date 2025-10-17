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
RNG_SEED="0000000000000000000000000000000000000000000000000000000000000037"

echo "🎯 Running Toolkit E2E test"
echo "🧱 NODE_IMAGE: $NODE_IMAGE"
echo "🧱 TOOLKIT_IMAGE: $TOOLKIT_IMAGE"

# Start node in background
echo "🚀 Starting node container..."
docker run -d --rm \
  --name midnight-node-tx \
  -e CFG_PRESET=dev \
  -e SIDECHAIN_BLOCK_BENEFICIARY="04bcf7ad3be7a5c790460be82a713af570f22e0f801f6659ab8e84a52be6969e" \
  "$NODE_IMAGE"

tempdir=$(mktemp -d 2>/dev/null || mktemp -d -t 'txgene2e')
cleanup() {
    echo "🛑 Killing node container..."
    docker container stop midnight-node-tx
    echo "🧹 Removing tempdir..."
    rm -rf $tempdir
}
# --- Always-cleanup: runs on success, error, or interrupt ---
trap cleanup EXIT

echo "⏳ Waiting for node to boot..."
sleep 10

# Run toolkit commands
echo "📦 Running toolkit tests..."

echo "Get version for toolkit"
docker run --rm -e RUST_BACKTRACE=1 "$TOOLKIT_IMAGE" version

deploy_filename="contract_deploy.mn"

docker run --rm -e RESTORE_OWNER="$(id -u):$(id -g)" -e RUST_BACKTRACE=1 --network container:midnight-node-tx "$TOOLKIT_IMAGE" generate-txs batches -n 1 -b 1

docker run --rm -e RESTORE_OWNER="$(id -u):$(id -g)" -e RUST_BACKTRACE=1 -v $tempdir:/out --network container:midnight-node-tx "$TOOLKIT_IMAGE" generate-txs \
    --dest-file "/out/$deploy_filename" --to-bytes \
    contract-simple deploy \
    --rng-seed "$RNG_SEED"

contract_address=$(
    docker run --rm -e RESTORE_OWNER="$(id -u):$(id -g)" -e RUST_BACKTRACE=1 -v $tempdir:/out "$TOOLKIT_IMAGE" \
        contract-address --src-file "/out/$deploy_filename" --tagged
)

docker run --rm -e RESTORE_OWNER="$(id -u):$(id -g)" -e RUST_BACKTRACE=1 -v $tempdir:/out --network container:midnight-node-tx "$TOOLKIT_IMAGE" generate-txs \
    --src-file="/out/$deploy_filename" send

docker run --rm -e RESTORE_OWNER="$(id -u):$(id -g)" -e RUST_BACKTRACE=1 -v $tempdir:/out --network container:midnight-node-tx "$TOOLKIT_IMAGE" \
    generate-txs contract-simple maintenance \
    --rng-seed "$RNG_SEED" \
    --contract-address "$contract_address"

docker run --rm -e RESTORE_OWNER="$(id -u):$(id -g)" -e RUST_BACKTRACE=1 -v $tempdir:/out --network container:midnight-node-tx "$TOOLKIT_IMAGE" \
    generate-txs contract-simple call \
    --call-key store \
    --rng-seed "$RNG_SEED" \
    --contract-address "$contract_address"

docker run --rm -e RESTORE_OWNER="$(id -u):$(id -g)" -e RUST_BACKTRACE=1 -v $tempdir:/out --network container:midnight-node-tx "$TOOLKIT_IMAGE" \
    generate-txs contract-simple call \
    --call-key check \
    --rng-seed "$RNG_SEED" \
    --contract-address "$contract_address"

# Send just unshielded
docker run --rm -e RUST_BACKTRACE=1 --network container:midnight-node-tx "$TOOLKIT_IMAGE" \
    generate-txs single-tx \
    --source-seed "0000000000000000000000000000000000000000000000000000000000000001" \
    --unshielded-amount 10 \
    --destination-address mn_addr_dev1m008urkd83umdn3j2nznwyrp34ug5negs2tawcgvcxnmchx7v60qr7c804

echo "✅ Toolkit E2E"
