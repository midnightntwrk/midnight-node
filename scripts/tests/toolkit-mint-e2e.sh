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

# Usage:
#   ./toolkit-mint-e2e.sh <NODE_IMAGE> <TOOLKIT_IMAGE>   # Docker mode (CI): starts the node
#                                                        # container and runs the toolkit image.
#   ./toolkit-mint-e2e.sh                                # Local mode: uses the locally-built
#                                                        # toolkit binary (./target/debug/midnight-node-toolkit)
#                                                        # and the mint contract under util/toolkit-js/mint.
#                                                        # Requires a node already listening on localhost:9944.

set -euxo pipefail

# shellcheck disable=SC1091
. "$(dirname "$0")/lib/wait-for-node.sh"

echo "🎯 Running Toolkit Mint test"

contract_dir="contract"

docker_outdir="/out"
docker_toolkit_js_path="/toolkit-js"
docker_config_file="/toolkit-js/contract/mint.config.ts"
docker_compiled_contract="/toolkit-js/contract/out"

local_outdir="out"
local_toolkit_js_path="$PWD/util/toolkit-js"
local_config_file="util/toolkit-js/mint/mint.config.ts"
local_compiled_contract="util/toolkit-js/mint/out"
local_toolkit_bin="./target/debug/midnight-node-toolkit"

if [[ "${1-}" != "" && "${2-}" != "" ]]; then
    NODE_IMAGE="$1"
    TOOLKIT_IMAGE="$2"
    mode="docker"
    outdir=$docker_outdir
    toolkit_js_path=$docker_toolkit_js_path
    config_file=$docker_config_file
    compiled_contract=$docker_compiled_contract

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

    wait_for_unfinalized_block http://localhost:9944 1

    tempdir=$(mktemp -d 2>/dev/null || mktemp -d -t 'toolkitcontracts')

    cleanup() {
        echo "🛑 Killing node container..."
        docker container stop midnight-node-contracts
        echo "🧹 Removing tempdir..."
        rm -rf "$tempdir"
    }
    # --- Always-cleanup: runs on success, error, or interrupt ---
    trap cleanup EXIT

    # Compiled mint contract is included in the toolkit image
    tmpid=$(docker create "$TOOLKIT_IMAGE")
    docker cp "$tmpid:/toolkit-js/mint" "$tempdir/$contract_dir"
    docker rm -v "$tmpid"
else
    echo "🧱 NODE_IMAGE: local"
    echo "🧱 TOOLKIT_IMAGE: local"

    mode="local"
    outdir=$local_outdir
    toolkit_js_path=$local_toolkit_js_path
    tempdir=$outdir
    config_file=$local_config_file
    compiled_contract=$local_compiled_contract

    mkdir -p $outdir
fi

toolkit() {
    if [ "$mode" = "docker" ]; then
        docker run --rm -e RUST_BACKTRACE=1 --network container:midnight-node-contracts \
            -e RESTORE_OWNER="$(id -u):$(id -g)" \
            -v "$tempdir":/out -v "$tempdir/$contract_dir":/toolkit-js/contract \
            "$TOOLKIT_IMAGE" "$@"
    else
        "$local_toolkit_bin" "$@"
    fi
}

# Run toolkit commands
echo "📦 Running toolkit contract tests..."

deploy_intent_filename="deploy.bin"
deploy_tx_filename="deploy_tx.mn"
deploy_zswap_filename="deploy_zswap.json"

private_state_filename="state.json"

state_filename="contract_state.mn"

mint_intent_filename="mint.bin"
mint_zswap_filename="mint_zswap.json"

coin_public=$(
    toolkit show-address \
    --network undeployed \
    --seed 0000000000000000000000000000000000000000000000000000000000000001 \
    --coin-public
)

echo "Generate deploy intent"
toolkit \
    generate-intent deploy \
    -c "$config_file" \
    --toolkit-js-path "$toolkit_js_path" \
    --output-intent "$outdir/$deploy_intent_filename" \
    --output-private-state "$outdir/$private_state_filename" \
    --output-zswap-state "$outdir/$deploy_zswap_filename" \
    --coin-public "$coin_public"

test -f "$tempdir/$deploy_intent_filename"
test -f "$tempdir/$private_state_filename"

echo "Generate deploy tx"
toolkit \
    send-intent --intent-file "$outdir/$deploy_intent_filename" \
    --compiled-contract-dir "$compiled_contract" \
    --dest-file "$outdir/$deploy_tx_filename"

echo "Send deploy tx"
toolkit generate-txs --src-file "$outdir/$deploy_tx_filename" -r 1 send

contract_address=$(
    toolkit \
    contract-address \
    --src-file "$outdir/$deploy_tx_filename"
)

echo "Get contract state"
toolkit \
    contract-state --contract-address "$contract_address" \
    --dest-file "$outdir/$state_filename"

nonce="3337000000000000000000000000000000000000000000000000000000000000"
domain_sep="feeb000000000000000000000000000000000000000000000000000000000000"

echo "Generate circuit call intent"
toolkit \
    generate-intent circuit -c "$config_file" \
    --toolkit-js-path "$toolkit_js_path" \
    --input-onchain-state "$outdir/$state_filename" --input-private-state "$outdir/$private_state_filename" \
    --contract-address "$contract_address" \
    --output-intent "$outdir/$mint_intent_filename" \
    --output-private-state "$outdir/tmp.json" \
    --output-zswap-state "$outdir/$mint_zswap_filename" \
    --coin-public "$coin_public" \
    mint \
    "$nonce" \
    "$domain_sep" \
    1000

shielded_destination=$(
    toolkit show-address \
    --network undeployed \
    --seed 0000000000000000000000000000000000000000000000000000000000000001 \
    --shielded
)

echo "Generate and send mint tx"
toolkit \
    send-intent \
    --intent-file "$outdir/$mint_intent_filename" \
    --zswap-state-file "$outdir/$mint_zswap_filename" \
    --compiled-contract-dir "$compiled_contract" \
    --shielded-destination "$shielded_destination"

token_type=$(
    toolkit show-token-type \
    --contract-address "$contract_address" \
    --domain-sep "$domain_sep" \
    --unshielded
)

show_wallet_output=$(
    toolkit show-wallet --seed "0000000000000000000000000000000000000000000000000000000000000001"
)

if echo "$show_wallet_output" | grep -q "$token_type"; then
    echo "🕵️✅ Found matching shielded coin"
else
    echo "🕵️❌ Couldn't find matching shielded coin"
    exit 1
fi

echo "✅ Toolkit Mint"
