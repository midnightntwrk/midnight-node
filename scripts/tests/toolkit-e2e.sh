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
    --dest-file "/out/$deploy_filename" \
    contract-simple deploy \
    --rng-seed "$RNG_SEED" \
    -s ws://midnight-node-tx:9944

contract_address=$(
    docker run --rm -e RESTORE_OWNER="$(id -u):$(id -g)" -e RUST_BACKTRACE=1 -v $tempdir:/out "$TOOLKIT_IMAGE" \
        contract-address --src-file "/out/$deploy_filename"
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
    --destination-address mn_addr_undeployed1gkasr3z3vwyscy2jpp53nzr37v7n4r3lsfgj6v5g584dakjzt0xqun4d4r \
    --destination-address mn_addr_undeployed1g9nr3mvjcey7ca8shcs5d4yjndcnmczf90rhv4nju7qqqlfg4ygs0t4ngm \
    --destination-address mn_addr_undeployed12vv6yst6exn50pkjjq54tkmtjpyggmr2p07jwpk6pxd088resqzqszfgak \
    -s ws://midnight-node-tx:9944 \
    -d ws://midnight-node-tx:9944

dust_address=$(
    docker run --rm -e RUST_BACKTRACE=1 "$TOOLKIT_IMAGE" \
    show-address \
    --network undeployed \
    --seed 0000000000000000000000000000000000000000000000000000000000000002 \
    --dust
)

echo "Register received tokens..."
docker run --rm -e RUST_BACKTRACE=1 --network toolkit-e2e-net "$TOOLKIT_IMAGE" \
    generate-txs register-dust-address \
    --wallet-seed "0000000000000000000000000000000000000000000000000000000000000002" \
    --funding-seed "0000000000000000000000000000000000000000000000000000000000000002" \
    --destination-dust "$dust_address" \
    -s ws://midnight-node-tx:9944 \
    -d ws://midnight-node-tx:9944

echo "Register empty wallet..."
docker run --rm -e RUST_BACKTRACE=1 --network toolkit-e2e-net "$TOOLKIT_IMAGE" \
    generate-txs register-dust-address \
    --wallet-seed "0000000000000000000000000000000000000000000000000000000000000052" \
    --funding-seed "0000000000000000000000000000000000000000000000000000000000000002" \
    -s ws://midnight-node-tx:9944 \
    -d ws://midnight-node-tx:9944

echo "Deregister dust address..."
docker run --rm -e RUST_BACKTRACE=1 --network toolkit-e2e-net "$TOOLKIT_IMAGE" \
    generate-txs deregister-dust-address \
    --wallet-seed "0000000000000000000000000000000000000000000000000000000000000002" \
    --funding-seed "0000000000000000000000000000000000000000000000000000000000000002" \
    -s ws://midnight-node-tx:9944 \
    -d ws://midnight-node-tx:9944

echo "Sending just shielded tokens..."
docker run --rm -e RUST_BACKTRACE=1 --network toolkit-e2e-net "$TOOLKIT_IMAGE" \
    generate-txs single-tx \
    --source-seed "0000000000000000000000000000000000000000000000000000000000000001" \
    --shielded-amount 10 \
    --destination-address mn_shield-addr_undeployed1tdu4jzhm7xn9qhzwweleyszxmhtt7fnzfhql42g87aay2jdjvau3fljgum7nqky8cj5mmm697rd33uyh6dnw42thuucjp7da74nje0sggh42d \
    --destination-address mn_shield-addr_undeployed1tth9g6jf8he6cmhgtme6arty0jde7wnypsg53qc3x5navl9za355jqqvfftm8asg986dx9puzwkmedeune9nfkuqvtmccmxtjwvlrvccwypcs \
    --destination-address mn_shield-addr_undeployed1ngp7ce7cqclgucattj5kuw68v3s4826e9zwalhhmurymwet3v7psvrs4gtpv5p2zx8rd3jxpgjr4m8mxh7js7u3l33g23gcty67uq9cug4xep \
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
