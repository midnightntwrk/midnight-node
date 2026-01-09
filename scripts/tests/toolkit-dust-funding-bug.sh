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

echo "🎯 Running Toolkit Dust Funding bug replication test"
echo "🧱 NODE_IMAGE: $NODE_IMAGE"
echo "🧱 TOOLKIT_IMAGE: $TOOLKIT_IMAGE"

# Ensure Docker network exists
docker network create midnight-net-contracts || true

# Start node in background
echo "🚀 Starting node container..."
docker run -d --rm \
  --name midnight-node-contracts \
  --network midnight-net-contracts \
  -p 9944:9944 \
  -e CFG_PRESET=dev \
  -e SIDECHAIN_BLOCK_BENEFICIARY="04bcf7ad3be7a5c790460be82a713af570f22e0f801f6659ab8e84a52be6969e" \
  "$NODE_IMAGE"


tempdir=$(mktemp -d 2>/dev/null || mktemp -d -t 'toolkitcontracts')
cleanup() {
    echo "🛑 Killing node container..."
    docker container stop midnight-node-contracts
    echo "🧹 Removing tempdir..."
    rm -rf $tempdir
}
# --- Always-cleanup: runs on success, error, or interrupt ---
trap cleanup EXIT

echo "⏳ Waiting for node to boot..."
sleep 20

echo "00..01, 00..02, and 00..03 are pre-funded with NIGHT + DUST"

destination_unshielded_07=$(
    docker run --rm -e RUST_BACKTRACE=1 "$TOOLKIT_IMAGE" \
        show-address \
        --seed 00..07 \
        --network undeployed \
        --unshielded
)
destination_unshielded_08=$(
    docker run --rm -e RUST_BACKTRACE=1 "$TOOLKIT_IMAGE" \
        show-address \
        --seed 00..08 \
        --network undeployed \
        --unshielded
)

echo "Send 10 STAR to 00..07"
docker run --rm --network midnight-net-contracts -e RUST_BACKTRACE=1 "$TOOLKIT_IMAGE" \
    generate-txs \
    single-tx \
    -s ws://midnight-node-contracts:9944 \
    -d ws://midnight-node-contracts:9944 \
    --source-seed 00..01 \
    --destination-address "$destination_unshielded_07" \
    --unshielded-amount 10

dust_balance_before_01=$(
    docker run --rm --network midnight-net-contracts -e RUST_BACKTRACE=1 "$TOOLKIT_IMAGE" \
        dust-balance \
        -s ws://midnight-node-contracts:9944 \
        --seed 00..01 | jq -r '.total'
)


echo "Send 10 STAR from 00..07 to 00..08, funded by 00..01"
docker run --rm --network midnight-net-contracts -e RUST_BACKTRACE=1 "$TOOLKIT_IMAGE" \
    generate-txs \
    single-tx \
    -s ws://midnight-node-contracts:9944 \
    -d ws://midnight-node-contracts:9944 \
    --source-seed 00..07 \
    --funding-seed 00..01 \
    --destination-address "$destination_unshielded_08" \
    --unshielded-amount 10

dust_balance_after_01=$(
    docker run --rm --network midnight-net-contracts -e RUST_BACKTRACE=1 "$TOOLKIT_IMAGE" \
        dust-balance \
        -s ws://midnight-node-contracts:9944 \
        --seed 00..01 | jq -r '.total'
)

if [ "$dust_balance_before_01" == "$dust_balance_after_01" ]; then
    echo "❌ Error: Dust balance of funding wallet 00..01 has not changed"
    exit 1
else
    echo "✅ Funding wallet Dust balance changed: Dust balance before: $dust_balance_before_01, Dust balance after: $dust_balance_after_01"
fi

dust_balance_before_03=$(
    docker run --rm --network midnight-net-contracts -e RUST_BACKTRACE=1 "$TOOLKIT_IMAGE" \
        dust-balance \
        -s ws://midnight-node-contracts:9944 \
        --seed 00..03 | jq -r '.total'
)

echo "Send 10 STAR from 00..02 to 00..08, funded by 00..03"
docker run --rm --network midnight-net-contracts -e RUST_BACKTRACE=1 "$TOOLKIT_IMAGE" \
    generate-txs \
    single-tx \
    -s ws://midnight-node-contracts:9944 \
    -d ws://midnight-node-contracts:9944 \
    --source-seed 00..02 \
    --funding-seed 00..03 \
    --destination-address "$destination_unshielded_08" \
    --unshielded-amount 10

dust_balance_after_03=$(
    docker run --rm --network midnight-net-contracts -e RUST_BACKTRACE=1 "$TOOLKIT_IMAGE" \
        dust-balance \
        -s ws://midnight-node-contracts:9944 \
        --seed 00..03 | jq -r '.total'
)

if [ "$dust_balance_before_03" == "$dust_balance_after_03" ]; then
    echo "❌ Error: Dust balance of funding wallet 00..03 has not changed"
    exit 1
else
    echo "✅ Funding wallet Dust balance changed: Dust balance before: $dust_balance_before_03, Dust balance after: $dust_balance_after_03"
fi

echo "✅ Failed to replicate Toolkit Dust Funding Bug"
