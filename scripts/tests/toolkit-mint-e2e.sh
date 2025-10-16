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

echo "🎯 Running Toolkit Mint test"
echo "🧱 NODE_IMAGE: $NODE_IMAGE"
echo "🧱 TOOLKIT_IMAGE: $TOOLKIT_IMAGE"

# Start node in background
echo "🚀 Starting node container..."
docker run -d --rm \
  --name midnight-node-contracts \
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
sleep 5

# Run toolkit commands
echo "📦 Running toolkit contract tests..."

deploy_intent_filename="deploy.bin"
deploy_tx_filename="deploy_tx.mn"
deploy_zswap_filename="deploy_zswap.json"

private_state_filename="state.json"

address_filename="contract_address.mn"
state_filename="contract_state.mn"

mint_intent_filename="mint.bin"
mint_tx_filename="mint_tx.mn"
mint_zswap_filename="mint_zswap.json"

contract_dir="contract"

# Compiled mint contract is included in the toolkit image
tmpid=$(docker create "$TOOLKIT_IMAGE")
docker cp "$tmpid:/toolkit-js/mint" "$tempdir/$contract_dir"
docker rm -v $tmpid

coin_public=$(
    docker run --rm -e RUST_BACKTRACE=1 "$TOOLKIT_IMAGE" \
    show-address \
    --network undeployed \
    --seed 0000000000000000000000000000000000000000000000000000000000000001 \
    --coin-public
)

echo "Generate deploy intent"
docker run --rm -e RUST_BACKTRACE=1 --network container:midnight-node-contracts \
    -e RESTORE_OWNER="$(id -u):$(id -g)" \
    -v $tempdir:/out -v $tempdir/$contract_dir:/toolkit-js/contract \
    "$TOOLKIT_IMAGE" \
    generate-intent deploy \
    -c /toolkit-js/contract/mint.config.ts \
    --output-intent "/out/$deploy_intent_filename" \
    --output-private-state "/out/$private_state_filename" \
    --output-zswap-state "/out/$deploy_zswap_filename" \
    --coin-public "$coin_public"

test -f "$tempdir/$deploy_intent_filename"
test -f "$tempdir/$private_state_filename"

echo "Generate deploy tx"
docker run --rm -e RUST_BACKTRACE=1 --network container:midnight-node-contracts \
    -e RESTORE_OWNER="$(id -u):$(id -g)" \
    -v $tempdir:/out -v $tempdir/$contract_dir:/toolkit-js/contract \
    "$TOOLKIT_IMAGE" \
    send-intent --intent-file "/out/$deploy_intent_filename" \
    --compiled-contract-dir contract/managed/counter \
    --to-bytes --dest-file "/out/$deploy_tx_filename"

echo "Send deploy tx"
docker run --rm -e RUST_BACKTRACE=1 --network container:midnight-node-contracts \
    -e RESTORE_OWNER="$(id -u):$(id -g)" \
    -v $tempdir:/out -v $tempdir/$contract_dir:/toolkit-js/contract \
    "$TOOLKIT_IMAGE" \
    generate-txs --src-file /out/$deploy_tx_filename -r 1 send

contract_address=$(
    docker run --rm -e RUST_BACKTRACE=1 --network container:midnight-node-contracts \
    -e RESTORE_OWNER="$(id -u):$(id -g)" \
    -v $tempdir:/out -v $tempdir/$contract_dir:/toolkit-js/contract \
    "$TOOLKIT_IMAGE" \
    contract-address \
    --src-file /out/$deploy_tx_filename
)

echo "Get contract state"
docker run --rm -e RUST_BACKTRACE=1 --network container:midnight-node-contracts \
    -e RESTORE_OWNER="$(id -u):$(id -g)" \
    -v $tempdir:/out -v $tempdir/$contract_dir:/toolkit-js/contract \
    "$TOOLKIT_IMAGE" \
    contract-state --contract-address $contract_address \
    --dest-file /out/$state_filename

nonce=$(echo "3337000000000000000000000000000000000000000000000000000000000000")
domain_sep=$(echo "feeb000000000000000000000000000000000000000000000000000000000000")

echo "Generate circuit call intent"
docker run --rm -e RUST_BACKTRACE=1 --network container:midnight-node-contracts \
    -e RESTORE_OWNER="$(id -u):$(id -g)" \
    -v $tempdir:/out -v $tempdir/$contract_dir:/toolkit-js/contract \
    "$TOOLKIT_IMAGE" \
    generate-intent circuit -c /toolkit-js/contract/mint.config.ts \
    --input-onchain-state "/out/$state_filename" --input-private-state "/out/$private_state_filename" \
    --contract-address $contract_address \
    --output-intent "/out/$mint_intent_filename" \
    --output-private-state "/out/tmp.json" \
    --output-zswap-state "/out/$mint_zswap_filename" \
    --coin-public "$coin_public" \
    mint \
    "$nonce" \
    "$domain_sep" \
    1000

shielded_destination=$(
    docker run --rm -e RUST_BACKTRACE=1 "$TOOLKIT_IMAGE" \
    show-address \
    --network undeployed \
    --seed 0000000000000000000000000000000000000000000000000000000000000001 \
    --shielded
)

echo "Generate and send mint tx"
docker run --rm -e RUST_BACKTRACE=1 --network container:midnight-node-contracts \
    -e RESTORE_OWNER="$(id -u):$(id -g)" \
    -v $tempdir:/out -v $tempdir/$contract_dir:/toolkit-js/contract \
    "$TOOLKIT_IMAGE" \
    send-intent \
    --intent-file "/out/$mint_intent_filename" \
    --zswap-state-file "/out/$mint_zswap_filename" \
    --compiled-contract-dir /toolkit-js/contract/out \
    --shielded-destination "$shielded_destination"

token_type=$(
    docker run --rm -e RUST_BACKTRACE=1 "$TOOLKIT_IMAGE" \
    show-token-type \
    --contract-address "$contract_address" \
    --domain-sep "$domain_sep" \
    --unshielded
)

show_wallet_output=$(docker run --rm -e RUST_BACKTRACE=1 --network container:midnight-node-contracts $TOOLKIT_IMAGE \
    show-wallet --seed "0000000000000000000000000000000000000000000000000000000000000001")

if echo "$show_wallet_output" | grep -q "$token_type"; then
    echo "🕵️✅ Found matching shielded coin"
else
    echo "🕵️❌ Couldn't find matching shielded coin"
    exit 1
fi

# rm -rf $tempdir

echo "✅ Toolkit Mint"
