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

# Mint shielded tokens to the faucet wallet (seed 0x..01), using the toolkit's bundled
# mint contract. The faucet only drips once its shielded wallet reports synced, and
# wallet-sdk-shielded never does on a chain with no zswap events: midnightntwrk/midnight-wallet#924.

set -euo pipefail

TOOLKIT=/midnight-node-toolkit
NODE_URL="ws://${NODE_HOST:-midnight-node-1}:${NODE_PORT:-9933}"
FAUCET_SEED="${FAUCET_SEED:-0000000000000000000000000000000000000000000000000000000000000001}"
MARKER_FILE="${MARKER_FILE:-/runtime-values/shielded-faucet-ready}"
CONFIG=/toolkit-js/mint/mint.config.ts
COMPILED=/toolkit-js/mint/out
OUT=$(mktemp -d)

if [ -s "$MARKER_FILE" ]; then
  echo "Shielded faucet already initialized ($MARKER_FILE present); skipping."
  exit 0
fi

echo "=== init shielded faucet (wallet seed ${FAUCET_SEED:0:6}..${FAUCET_SEED: -2}) ==="

coin_public=$("$TOOLKIT" show-address --network local --seed "$FAUCET_SEED" --coin-public)
shielded_address=$("$TOOLKIT" show-address --network local --seed "$FAUCET_SEED" --shielded)

echo "Deploying the mint contract..."
"$TOOLKIT" generate-intent deploy -c "$CONFIG" --toolkit-js-path /toolkit-js --network local \
  --coin-public "$coin_public" --output-intent "$OUT/deploy.bin" \
  --output-private-state "$OUT/state.json" --output-zswap-state "$OUT/deploy_zswap.json"
"$TOOLKIT" send-intent --src-url "$NODE_URL" --funding-seed "$FAUCET_SEED" \
  --intent-file "$OUT/deploy.bin" --compiled-contract-dir "$COMPILED" --dest-file "$OUT/deploy_tx.mn"
"$TOOLKIT" generate-txs --src-file "$OUT/deploy_tx.mn" --dest-url "$NODE_URL" -r 1 send
contract=$("$TOOLKIT" contract-address --src-file "$OUT/deploy_tx.mn")
"$TOOLKIT" contract-state --src-url "$NODE_URL" --contract-address "$contract" \
  --dest-file "$OUT/contract_state.mn"

echo "Minting shielded tokens to $shielded_address..."
"$TOOLKIT" generate-intent circuit -c "$CONFIG" --toolkit-js-path /toolkit-js --network local \
  --src-url "$NODE_URL" --contract-address "$contract" --coin-public "$coin_public" \
  --input-onchain-state "$OUT/contract_state.mn" --input-private-state "$OUT/state.json" \
  --output-intent "$OUT/mint.bin" --output-private-state "$OUT/mint_state.json" \
  --output-zswap-state "$OUT/mint_zswap.json" --output-events "$OUT/mint_events.json" \
  mint \
  3337000000000000000000000000000000000000000000000000000000000000 \
  feeb000000000000000000000000000000000000000000000000000000000000 \
  1000
"$TOOLKIT" send-intent --src-url "$NODE_URL" --dest-url "$NODE_URL" --funding-seed "$FAUCET_SEED" \
  --intent-file "$OUT/mint.bin" --zswap-state-file "$OUT/mint_zswap.json" \
  --compiled-contract-dir "$COMPILED" --shielded-destination "$shielded_address"

echo "shielded tokens minted by contract $contract" > "$MARKER_FILE"
echo "=== shielded faucet ready ==="
