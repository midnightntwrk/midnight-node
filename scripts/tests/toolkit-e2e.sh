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

# Ensure Docker network exists
docker network create toolkit-e2e-net || true

export POSTGRES_PASSWORD=$(uuidgen | tr -d '-' | head -c 16)

# Start a postgres container for the toolkit sync-cache
docker run -d --rm \
    --name postgres-test \
    --network toolkit-e2e-net \
    -e POSTGRES_USER=test \
    -e POSTGRES_PASSWORD \
    -e POSTGRES_DB=toolkit \
    postgres:16

# Start node in background
echo "🚀 Starting node container..."
docker run -d --rm \
  --name midnight-node-tx \
  --network toolkit-e2e-net \
  -e CFG_PRESET=dev \
  -e SIDECHAIN_BLOCK_BENEFICIARY="04bcf7ad3be7a5c790460be82a713af570f22e0f801f6659ab8e84a52be6969e" \
  "$NODE_IMAGE"

tempdir=$(mktemp -d 2>/dev/null || mktemp -d -t 'txgene2e')
cleanup() {
    echo "🛑 Killing node container..."
    docker container stop midnight-node-tx
    docker container stop postgres-test
    echo "🧹 Removing tempdir..."
    rm -rf $tempdir
}
# --- Always-cleanup: runs on success, error, or interrupt ---
trap cleanup EXIT

echo "⏳ Waiting for node to boot... (allow at least 2 blocks to be produced)"
sleep 20

# Run toolkit commands
echo "📦 Running toolkit tests..."

echo "Get version for toolkit"
docker run --rm -e RUST_BACKTRACE=1 --network toolkit-e2e-net "$TOOLKIT_IMAGE" version

deploy_filename="contract_deploy.mn"

docker run --rm -e RESTORE_OWNER="$(id -u):$(id -g)" \
    -e RUST_BACKTRACE=1 \
    --network toolkit-e2e-net \
    "$TOOLKIT_IMAGE" \
    generate-txs batches -n 1 -b 1 \
    -s ws://midnight-node-tx:9944 \
    -d ws://midnight-node-tx:9944

docker run --rm -e RESTORE_OWNER="$(id -u):$(id -g)" -e RUST_BACKTRACE=1 -v $tempdir:/out --network toolkit-e2e-net "$TOOLKIT_IMAGE" generate-txs \
    --dest-file "/out/$deploy_filename" --to-bytes \
    contract-simple deploy \
    --rng-seed "$RNG_SEED" \
    -s ws://midnight-node-tx:9944

contract_address=$(
    docker run --rm -e RESTORE_OWNER="$(id -u):$(id -g)" -e RUST_BACKTRACE=1 -v $tempdir:/out "$TOOLKIT_IMAGE" \
        contract-address --src-file "/out/$deploy_filename" --tagged
)

docker run --rm -e RESTORE_OWNER="$(id -u):$(id -g)" -e RUST_BACKTRACE=1 -v $tempdir:/out --network toolkit-e2e-net "$TOOLKIT_IMAGE" generate-txs \
    --src-file="/out/$deploy_filename" send \
    -d ws://midnight-node-tx:9944

docker run --rm -e RESTORE_OWNER="$(id -u):$(id -g)" -e RUST_BACKTRACE=1 -v $tempdir:/out --network toolkit-e2e-net "$TOOLKIT_IMAGE" \
    generate-txs contract-simple maintenance \
    --rng-seed "$RNG_SEED" \
    --contract-address "$contract_address" \
    --new-authority-seed 1000000000000000000000000000000000000000000000000000000000000001 \
    -s ws://midnight-node-tx:9944 \
    -d ws://midnight-node-tx:9944

docker run --rm -e RESTORE_OWNER="$(id -u):$(id -g)" -e RUST_BACKTRACE=1 -v $tempdir:/out --network toolkit-e2e-net "$TOOLKIT_IMAGE" \
    generate-txs contract-simple call \
    --call-key store \
    --rng-seed "$RNG_SEED" \
    --contract-address "$contract_address" \
    -s ws://midnight-node-tx:9944 \
    -d ws://midnight-node-tx:9944

docker run --rm -e RESTORE_OWNER="$(id -u):$(id -g)" -e RUST_BACKTRACE=1 -v $tempdir:/out --network toolkit-e2e-net "$TOOLKIT_IMAGE" \
    generate-txs contract-simple call \
    --call-key check \
    --rng-seed "$RNG_SEED" \
    --contract-address "$contract_address" \
    -s ws://midnight-node-tx:9944 \
    -d ws://midnight-node-tx:9944

echo "Sending just unshielded tokens..."
docker run --rm -e RUST_BACKTRACE=1 --network toolkit-e2e-net "$TOOLKIT_IMAGE" \
    generate-txs single-tx \
    --source-seed "0000000000000000000000000000000000000000000000000000000000000001" \
    --unshielded-amount 10 \
    --destination-address mn_addr_undeployed1na9c5lvmj6rvwkwkuq7vsqvyjcx74tg0th0vevrcluatxq02h9gsjtnn9j \
    -s ws://midnight-node-tx:9944 \
    -d ws://midnight-node-tx:9944

echo "Sending just shielded tokens..."
docker run --rm -e RUST_BACKTRACE=1 --network toolkit-e2e-net "$TOOLKIT_IMAGE" \
    generate-txs single-tx \
    --source-seed "0000000000000000000000000000000000000000000000000000000000000001" \
    --shielded-amount 10 \
    --destination-address mn_shield-addr_undeployed1tdu4jzhm7xn9qhzwweleyszxmhtt7fnzfhql42g87aay2jdjvau3fljgum7nqky8cj5mmm697rd33uyh6dnw42thuucjp7da74nje0sggh42d \
    -s ws://midnight-node-tx:9944 \
    -d ws://midnight-node-tx:9944

echo "Try fetching with all backends"

echo "fetching with redb"
docker run --rm -e RUST_BACKTRACE=1 --network toolkit-e2e-net "$TOOLKIT_IMAGE" \
    fetch --fetch-cache "redb:.cache/fetch/e2e_test.db" \
    -s ws://midnight-node-tx:9944


echo "fetching with inmemory"
docker run --rm -e RUST_BACKTRACE=1 --network toolkit-e2e-net "$TOOLKIT_IMAGE" \
    fetch --fetch-cache "inmemory" \
    -s ws://midnight-node-tx:9944

echo "fetching with postgres"
docker run --rm -e RUST_BACKTRACE=1 --network toolkit-e2e-net "$TOOLKIT_IMAGE" \
    fetch --fetch-cache "postgres://test:$POSTGRES_PASSWORD@postgres-test:5432/toolkit" \
    -s ws://midnight-node-tx:9944

echo "✅ Toolkit E2E"
