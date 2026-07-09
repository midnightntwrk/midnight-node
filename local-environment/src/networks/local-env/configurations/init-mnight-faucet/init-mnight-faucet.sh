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

# Initialize the Midnight dev faucet wallet (seed 0x..01) on a fresh local-env:
#
#  1. wait for the bridge observation to surface the c2m faucet transfer (submitted
#     pre-genesis by mint-cnight-supply and pre-approved in the c2m-bridge genesis
#     config, so no governance round is needed);
#  2. claim it — `claim-rewards --claim-kind cardano-bridge` is feeless and self-signed
#     by the seed, so the otherwise-empty wallet needs no pre-existing balance or DUST;
#  3. register the wallet for DUST production — after the claimed NIGHT ages a few
#     blocks, the registration self-funds from retroactive DUST (no --funding-seed).

set -euo pipefail

TOOLKIT=/midnight-node-toolkit
NODE_HOST="${NODE_HOST:-midnight-node-1}"
NODE_PORT="${NODE_PORT:-9933}"
NODE_URL="ws://${NODE_HOST}:${NODE_PORT}"
FAUCET_SEED="${FAUCET_SEED:-0000000000000000000000000000000000000000000000000000000000000001}"
FAUCET_MARKER_FILE="${FAUCET_MARKER_FILE:-/runtime-values/mnight-faucet-ready}"

if [ -s "$FAUCET_MARKER_FILE" ]; then
  echo "Faucet already initialized ($FAUCET_MARKER_FILE present); skipping."
  exit 0
fi

echo "=== init mNIGHT faucet (wallet seed ${FAUCET_SEED:0:6}..${FAUCET_SEED: -2}) ==="

# 1. Wait for the node RPC to accept connections (depends_on service_started is not
#    RPC-ready) — /dev/tcp probe, no extra tooling needed.
echo "Waiting for $NODE_HOST:$NODE_PORT ..."
for i in {1..30}; do
  if (exec 3<>"/dev/tcp/${NODE_HOST}/${NODE_PORT}") 2>/dev/null; then
    exec 3>&- 3<&- || true
    echo "Node RPC is up."
    break
  fi
  [ "$i" -eq 30 ] && { echo "ERROR: node RPC not reachable after 1 min"; exit 1; }
  sleep 2
done

show_wallet() {
  "$TOOLKIT" show-wallet --src-url "$NODE_URL" --seed "$FAUCET_SEED" 2>/dev/null
}

# 2. Wait for the bridge observation to make the transfer claimable (the mainchain
#    follower needs the Cardano tx at stable depth + db-sync to have indexed it).
#    Local-env only, so 1 minute is the cap.
echo "Waiting for the faucet bridge transfer to become claimable..."
CLAIMABLE=0
for i in {1..20}; do
  CLAIMABLE=$(show_wallet | jq -r '.claimable_bridge_transfers // 0' || echo 0)
  if [ "$CLAIMABLE" != "0" ] && [ -n "$CLAIMABLE" ] && [ "$CLAIMABLE" != "null" ]; then
    echo "Claimable bridge transfer: $CLAIMABLE STARS"
    break
  fi
  [ "$i" -eq 20 ] && { echo "ERROR: no claimable bridge transfer after 1 min"; exit 1; }
  echo "  not claimable yet (attempt $i/20)..."
  sleep 3
done

# 3. Claim it (feeless, self-signed: funding_seed IS the claiming wallet's seed).
#    The default destination watcher waits for finalization before returning.
echo "Claiming $CLAIMABLE STARS from the Cardano bridge..."
RUST_LOG=info "$TOOLKIT" generate-txs \
  --src-url "$NODE_URL" --dest-url "$NODE_URL" \
  claim-rewards \
  --funding-seed "$FAUCET_SEED" \
  --amount "$CLAIMABLE" \
  --claim-kind cardano-bridge

# 4. Register for DUST production. The registration self-funds from the retroactive
#    DUST the claimed NIGHT generates as it ages (~1 block/6 s on local-env), so retry
#    a few times while it accrues instead of hardcoding an aging sleep.
echo "Registering wallet for DUST production..."
registered=false
for i in {1..10}; do
  if RUST_LOG=info "$TOOLKIT" generate-txs \
      --src-url "$NODE_URL" --dest-url "$NODE_URL" \
      register-dust-address \
      --wallet-seed "$FAUCET_SEED"; then
    registered=true
    break
  fi
  echo "  DUST registration not accepted yet (attempt $i/10); letting the NIGHT age..."
  sleep 6
done
[ "$registered" = true ] || { echo "ERROR: DUST registration failed after 10 attempts"; exit 1; }

echo "faucet wallet funded ($CLAIMABLE STARS) and DUST-registered" > "$FAUCET_MARKER_FILE"
echo "=== mNIGHT faucet ready ==="
