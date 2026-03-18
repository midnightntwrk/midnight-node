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

echo "🎯 Running Toolkit Tokens Minter test"
echo "🧱 NODE_IMAGE: $NODE_IMAGE"
echo "🧱 TOOLKIT_IMAGE: $TOOLKIT_IMAGE"

# Start node in background
echo "🚀 Starting node container..."
docker run -d --rm \
  --name midnight-node-contracts \
  -e CFG_PRESET=dev \
  -e SIDECHAIN_BLOCK_BENEFICIARY="04bcf7ad3be7a5c790460be82a713af570f22e0f801f6659ab8e84a52be6969e" \
  "$NODE_IMAGE"

echo "⏳ Waiting for node to boot..."
sleep 15

# Run toolkit commands
echo "📦 Running toolkit contract tests..."

tempdir=$(mktemp -d 2>/dev/null || mktemp -d -t 'toolkitcontracts')

cleanup() {
    echo "🛑 Killing node container..."
    docker container stop midnight-node-contracts
    echo "🧹 Removing tempdir..."
    rm -rf $tempdir
}
# Set up trap to cleanup on exit
trap cleanup EXIT

compiled_contract="/toolkit-js/contract/out"
contract_dir="contract"
outdir="/out"
state_filename="contract_state.mn"
config_file="/toolkit-js/contract/minter.config.ts"

mint_shielded_intent_filename="mint_shielded.bin"
mint_unshielded_intent_filename="mint_unshielded.bin"
send_unshielded_intent_filename="send_unshielded.bin"
set_token_intent_filename="send_token.bin"
receive_and_send_unshielded_intent_filename="receive_and_send_unshielded.bin"
mint_shielded_zswap_filename="mint_zswap_shielded.json"
mint_unshielded_zswap_filename="mint_zswap_unshielded.json"

initial_private_state_filename="initial_state.json"
deploy_zswap_filename="deploy_zswap.json"

deploy_intent_filename="deploy.bin"
deploy_tx_filename="deploy.mn"

# Compiled mint contract is included in the toolkit image
tmpid=$(docker create "$TOOLKIT_IMAGE")
docker cp "$tmpid:/toolkit-js/test/minter_contract" "$tempdir/$contract_dir"
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
    generate-intent deploy -c "$config_file" \
    --coin-public "$coin_public" \
    --output-intent "$outdir/$deploy_intent_filename" \
    --output-private-state "$outdir/$initial_private_state_filename" \
    --output-zswap-state "$outdir/$deploy_zswap_filename"

test -f "$tempdir/$deploy_intent_filename"
test -f "$tempdir/$initial_private_state_filename"

echo "Generate deploy tx"
docker run --rm -e RUST_BACKTRACE=1 --network container:midnight-node-contracts \
    -e RESTORE_OWNER="$(id -u):$(id -g)" \
    -v $tempdir:/out -v $tempdir/$contract_dir:/toolkit-js/contract \
    "$TOOLKIT_IMAGE" \
    send-intent \
    --intent-file "$outdir/$deploy_intent_filename" \
    --compiled-contract-dir $compiled_contract \
    --dest-file "$outdir/$deploy_tx_filename"

echo "Send deploy tx"
docker run --rm -e RUST_BACKTRACE=1 --network container:midnight-node-contracts \
    -e RESTORE_OWNER="$(id -u):$(id -g)" \
    -v $tempdir:/out -v $tempdir/$contract_dir:/toolkit-js/contract \
    "$TOOLKIT_IMAGE" \
    generate-txs --src-file $outdir/$deploy_tx_filename -r 1 send

contract_address=$(
    docker run --rm -e RUST_BACKTRACE=1 --network container:midnight-node-contracts \
      -e RESTORE_OWNER="$(id -u):$(id -g)" \
      -v $tempdir:/out -v $tempdir/$contract_dir:/toolkit-js/contract \
      "$TOOLKIT_IMAGE" \
      contract-address \
      --src-file $outdir/$deploy_tx_filename
)

echo "Get contract state"
docker run --rm -e RUST_BACKTRACE=1 --network container:midnight-node-contracts \
    -e RESTORE_OWNER="$(id -u):$(id -g)" \
    -v $tempdir:/out -v $tempdir/$contract_dir:/toolkit-js/contract \
    "$TOOLKIT_IMAGE" \
    contract-state \
    --contract-address $contract_address \
    --dest-file $outdir/$state_filename

test -f "$tempdir/$state_filename"

domain_sep=$(echo "feeb000000000000000000000000000000000000000000000000000000000000")

user_address=$(
    docker run --rm -e RUST_BACKTRACE=1 "$TOOLKIT_IMAGE" \
        show-address \
        --network undeployed \
        --seed 0000000000000000000000000000000000000000000000000000000000000001 \
        --unshielded
)
token_type=$(
    docker run --rm -e RUST_BACKTRACE=1 "$TOOLKIT_IMAGE" \
        show-token-type \
        --contract-address "$contract_address" \
        --domain-sep "$domain_sep" \
        --unshielded
)
shielded_destination=$(
    docker run --rm -e RUST_BACKTRACE=1 "$TOOLKIT_IMAGE" \
      show-address \
      --network undeployed \
      --seed 0000000000000000000000000000000000000000000000000000000000000001 \
      --shielded
)

echo "Generate intent to mint shielded token"
docker run --rm -e RUST_BACKTRACE=1 --network container:midnight-node-contracts \
    -e RESTORE_OWNER="$(id -u):$(id -g)" \
    -v $tempdir:/out -v $tempdir/$contract_dir:/toolkit-js/contract \
    "$TOOLKIT_IMAGE" \
    generate-intent circuit -c "$config_file" \
    --coin-public "$coin_public" \
    --input-onchain-state "$outdir/$state_filename" \
    --input-private-state "$outdir/$initial_private_state_filename" \
    --contract-address $contract_address \
    --output-intent "$outdir/$mint_shielded_intent_filename" \
    --output-onchain-state "$outdir/onchain_state_1.mn" \
    --output-private-state "$outdir/temp_shielded_private_state.json" \
    --output-zswap-state "$outdir/$mint_shielded_zswap_filename" \
    mintShieldedToSelfTest \
    "$domain_sep" \
    1000

echo "Generate intent to mint unshielded token"
docker run --rm -e RUST_BACKTRACE=1 --network container:midnight-node-contracts \
    -e RESTORE_OWNER="$(id -u):$(id -g)" \
    -v $tempdir:/out -v $tempdir/$contract_dir:/toolkit-js/contract \
    "$TOOLKIT_IMAGE" \
    generate-intent circuit -c "$config_file" \
    --coin-public "$coin_public" \
    --input-onchain-state "$outdir/onchain_state_1.mn" \
    --input-private-state "$outdir/temp_shielded_private_state.json" \
    --contract-address $contract_address \
    --output-intent "$outdir/$mint_unshielded_intent_filename" \
    --output-onchain-state "$outdir/onchain_state_2.mn" \
    --output-private-state "$outdir/temp_unshielded_private_state.json" \
    --output-zswap-state "$outdir/$mint_unshielded_zswap_filename" \
    mintUnshieldedToSelfTest \
    "$domain_sep" \
    1000

echo "Generate intent to send unshielded token"
docker run --rm -e RUST_BACKTRACE=1 --network container:midnight-node-contracts \
    -e RESTORE_OWNER="$(id -u):$(id -g)" \
    -v $tempdir:/out -v $tempdir/$contract_dir:/toolkit-js/contract \
    "$TOOLKIT_IMAGE" \
    generate-intent circuit -c "$config_file" \
    --coin-public "$coin_public" \
    --input-onchain-state "$outdir/onchain_state_2.mn" \
    --input-private-state "$outdir/temp_unshielded_private_state.json" \
    --contract-address $contract_address \
    --output-intent "$outdir/$send_unshielded_intent_filename" \
    --output-onchain-state "$outdir/onchain_state_3.mn" \
    --output-private-state "$outdir/temp_send_private_state.json" \
    --output-zswap-state "$outdir/$mint_unshielded_zswap_filename" \
    sendUnshieldedToUser \
    "$token_type" \
    "$user_address" \
    1000

echo "Send created intents (tx #1)"
docker run --rm -e RUST_BACKTRACE=1 --network container:midnight-node-contracts \
    -e RESTORE_OWNER="$(id -u):$(id -g)" \
    -v $tempdir:/out -v $tempdir/$contract_dir:/toolkit-js/contract \
    "$TOOLKIT_IMAGE" \
    send-intent \
    --intent-file "$outdir/$mint_shielded_intent_filename" \
    --intent-file "$outdir/$mint_unshielded_intent_filename" \
    --intent-file "$outdir/$send_unshielded_intent_filename" \
    --compiled-contract-dir "$compiled_contract" \
    --shielded-destination "$shielded_destination" \
    --zswap-state-file "$outdir/$mint_shielded_zswap_filename"

token_id=$(
    docker run --rm -e RUST_BACKTRACE=1 --network container:midnight-node-contracts \
     "$TOOLKIT_IMAGE" \
      show-wallet --seed "0000000000000000000000000000000000000000000000000000000000000001" \
      | jq -r ".utxos[] | select(.token_type == \"$token_type\") | .id"
)

echo "Generate setToken() circuit call"
docker run --rm -e RUST_BACKTRACE=1 --network container:midnight-node-contracts \
    -e RESTORE_OWNER="$(id -u):$(id -g)" \
    -v $tempdir:/out -v $tempdir/$contract_dir:/toolkit-js/contract \
    "$TOOLKIT_IMAGE" \
    generate-intent circuit -c "$config_file" \
    --coin-public "$coin_public" \
    --input-onchain-state "$outdir/onchain_state_3.mn" \
    --input-private-state "$outdir/temp_send_private_state.json" \
    --contract-address $contract_address \
    --output-intent "$outdir/$set_token_intent_filename" \
    --output-onchain-state "$outdir/onchain_state_4.mn" \
    --output-private-state "$outdir/temp_shielded_private_state.json" \
    --output-zswap-state "$outdir/$mint_unshielded_zswap_filename" \
    setToken \
    "$token_type" \
    1000 \
    "$user_address"

echo "Generate receiveAndSendUnshielded() circuit call"
docker run --rm -e RUST_BACKTRACE=1 --network container:midnight-node-contracts \
    -e RESTORE_OWNER="$(id -u):$(id -g)" \
    -v $tempdir:/out -v $tempdir/$contract_dir:/toolkit-js/contract \
    "$TOOLKIT_IMAGE" \
    generate-intent circuit -c "$config_file" \
    --coin-public "$coin_public" \
    --input-onchain-state "$outdir/onchain_state_4.mn" \
    --input-private-state "$outdir/temp_shielded_private_state.json" \
    --contract-address $contract_address \
    --output-intent "$outdir/$receive_and_send_unshielded_intent_filename" \
    --output-onchain-state "$outdir/onchain_state_5.mn" \
    --output-private-state "$outdir/temp_shielded_private_state_2.json" \
    --output-zswap-state "$outdir/$mint_unshielded_zswap_filename" \
    receiveAndSendUnshielded

echo "Send created intents (tx #2)"
docker run --rm -e RUST_BACKTRACE=1 --network container:midnight-node-contracts \
    -e RESTORE_OWNER="$(id -u):$(id -g)" \
    -v $tempdir:/out -v $tempdir/$contract_dir:/toolkit-js/contract \
    "$TOOLKIT_IMAGE" \
    send-intent \
    --intent-file "$outdir/$set_token_intent_filename" \
    --intent-file "$outdir/$receive_and_send_unshielded_intent_filename" \
    --compiled-contract-dir "$compiled_contract" \
    --shielded-destination "$shielded_destination" \
    --zswap-state-file "$outdir/$mint_unshielded_zswap_filename" \
    --input-utxo "$token_id"

show_wallet_output=$(
    docker run --rm -e RUST_BACKTRACE=1 --network container:midnight-node-contracts \
     "$TOOLKIT_IMAGE" \
      show-wallet --seed "0000000000000000000000000000000000000000000000000000000000000001"
)

if echo "$show_wallet_output" | grep -q "$token_type"; then
    echo "🕵️✅ Found matching shielded coin"
else
    echo "🕵️❌ Couldn't find matching shielded coin"
    exit 1
fi
