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

compiled_contract="util/toolkit-js/test/minter_contract/out"
outdir="out"
compactc_bin="~/.compact/bin/compactc"
toolkit_bin="./target/debug/midnight-node-toolkit"
state_filename="contract_state.mn"
config_file="util/toolkit-js/test/minter_contract/minter.config.ts"

call_private_state_filename="call_state.json"

mint_shielded_intent_filename="mint_shielded.bin"
mint_unshielded_intent_filename="mint_unshielded.bin"
send_unshielded_intent_filename="send_unshielded.bin"
mint_and_send_shielded_intent_filename="mint_and_send_shielded.bin"
mint_tx_filename="mint_tx.mn"
mint_shielded_zswap_filename="mint_zswap_shielded.json"
mint_unshielded_zswap_filename="mint_zswap_unshielded.json"
mint_and_send_shielded_zswap_filename="mint_and_send_zswap_unshielded.json"

initial_private_state_filename="initial_state.json"

deploy_intent_filename="deploy.bin"
deploy_tx_filename="deploy.mn"

mkdir -p $outdir

coin_public=$(
    $toolkit_bin \
    show-address \
    --network undeployed \
    --seed 0000000000000000000000000000000000000000000000000000000000000001 \
    --coin-public
)

echo "Generate deploy intent"
"$toolkit_bin" \
    generate-intent deploy -c "$config_file" \
    --toolkit-js-path "$PWD/util/toolkit-js" \
    --coin-public "$coin_public" \
    --output-intent "$outdir/$deploy_intent_filename" \
    --output-private-state "$outdir/$initial_private_state_filename" \
    --output-zswap-state "$outdir/temp.json"

test -f "$outdir/$deploy_intent_filename"

echo "Generate deploy tx"
"$toolkit_bin" \
    send-intent \
    --intent-file "$outdir/$deploy_intent_filename" \
    --compiled-contract-dir $compiled_contract \
    --dest-file "$outdir/$deploy_tx_filename"

echo "Send deploy tx"
"$toolkit_bin" generate-txs --src-file $outdir/$deploy_tx_filename -r 1 send

contract_address=$(
"$toolkit_bin" \
    contract-address \
    --src-file $outdir/$deploy_tx_filename
)

echo "Get contract state"
"$toolkit_bin" \
    contract-state \
    --contract-address $contract_address \
    --dest-file $outdir/$state_filename

test -f "$outdir/$state_filename"

domain_sep=$(echo "feeb000000000000000000000000000000000000000000000000000000000000")

user_address=$(
    "$toolkit_bin" \
        show-address \
        --network undeployed \
        --seed 0000000000000000000000000000000000000000000000000000000000000001 \
        --unshielded
)
token_type=$(
    "$toolkit_bin" \
        show-token-type \
        --contract-address "$contract_address" \
        --domain-sep "$domain_sep" \
        --unshielded
)
shielded_destination=$(
    "$toolkit_bin" \
      show-address \
      --network undeployed \
      --seed 0000000000000000000000000000000000000000000000000000000000000001 \
      --shielded
)

echo "Generate intent to mint shielded token"
"$toolkit_bin" \
    generate-intent circuit -c "$config_file" \
    --toolkit-js-path "$PWD/util/toolkit-js" \
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
#    mintShieldedToUserTest \
#    "$domain_sep" \
#    1000 \
#    "{ bytes: '$coin_public' }"

echo "Generate intent to mint unshielded token"
"$toolkit_bin" \
    generate-intent circuit -c "$config_file" \
    --toolkit-js-path "$PWD/util/toolkit-js" \
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

echo "Generate intent for the mintAndSendImmediate() circuit call"
"$toolkit_bin" \
    generate-intent circuit -c "$config_file" \
    --toolkit-js-path "$PWD/util/toolkit-js" \
    --coin-public "$coin_public" \
    --input-onchain-state "$outdir/onchain_state_2.mn" \
    --input-private-state "$outdir/$initial_private_state_filename" \
    --input-zswap-state "$outdir/$mint_shielded_zswap_filename" \
    --contract-address $contract_address \
    --output-intent "$outdir/$mint_and_send_shielded_intent_filename" \
    --output-onchain-state "$outdir/onchain_state_3.mn" \
    --output-private-state "$outdir/temp_mint_send_private_state.json" \
    --output-zswap-state "$outdir/$mint_and_send_shielded_zswap_filename" \
    mintAndSendImmediate \
    "$domain_sep" \
    2000 \
    1000 \
    "{ bytes: '$coin_public' }"

echo "Generate intent to send unshielded token"
"$toolkit_bin" \
    generate-intent circuit -c "$config_file" \
    --toolkit-js-path "$PWD/util/toolkit-js" \
    --coin-public "$coin_public" \
    --input-onchain-state "$outdir/onchain_state_3.mn" \
    --input-private-state "$outdir/temp_unshielded_private_state.json" \
    --input-zswap-state "$outdir/$mint_and_send_shielded_zswap_filename" \
    --contract-address $contract_address \
    --output-intent "$outdir/$send_unshielded_intent_filename" \
    --output-private-state "$outdir/temp_send_private_state.json" \
    --output-zswap-state "$outdir/$mint_unshielded_zswap_filename" \
    sendUnshieldedToUser \
    "$token_type" \
    "$user_address" \
    1000

echo "Send created txs"
"$toolkit_bin" \
    send-intent \
    --intent-file "$outdir/$mint_shielded_intent_filename" \
    --intent-file "$outdir/$mint_unshielded_intent_filename" \
    --intent-file "$outdir/$mint_and_send_shielded_intent_filename" \
    --intent-file "$outdir/$send_unshielded_intent_filename" \
    --compiled-contract-dir "$compiled_contract" \
    --shielded-destination "$shielded_destination" \
    --zswap-state-file "$outdir/$mint_unshielded_zswap_filename"

show_wallet_output=$(
  "$toolkit_bin" \
     show-wallet --seed "0000000000000000000000000000000000000000000000000000000000000001"
)

if echo "$show_wallet_output" | grep -q "$token_type"; then
    echo "🕵️✅ Found matching shielded coin"
else
    echo "🕵️❌ Couldn't find matching shielded coin"
    exit 1
fi
