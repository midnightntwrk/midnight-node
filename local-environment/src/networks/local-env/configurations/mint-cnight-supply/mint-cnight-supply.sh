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

# Seed the Cardano side of the cNIGHT <-> NIGHT bridge so the cross-chain pool
# invariants hold at genesis (see midnight-node#1773 / #1778).
#
# On local-env the Midnight side already carries the full NIGHT pools via the
# committed genesis state (M.R reserve / M.L locked / M.U unlocked), but the
# Cardano side starts with ZERO cNIGHT, so e.g. `M.U <= C.L` (1.2B+ <= 0) is
# violated from genesis. This step mints the cNIGHT supply and distributes it so
# that the Cardano pools mirror the Midnight pools:
#
#   C.R (Reserve validator) = M.R = reserve_pool
#   C.L (ICS validator)     = M.U = unlocked  (= S - M.R - M.L)
#   C.U (faucet/circulating) = M.L = locked_pool
#
# It MUST run after the reserve/ICS validators are deployed (so we know their
# real addresses) and BEFORE midnight-setup captures the bridge observation
# checkpoint (`initial_data_checkpoint`), so the bridge treats the seeded ICS
# cNIGHT as pre-existing locked supply instead of sweeping it to Treasury. The
# docker-compose dependency chain (contract-compiler -> mint-cnight-supply ->
# midnight-setup) guarantees both orderings.

set -euo pipefail

NETWORK_MAGIC=42
RUNTIME_VALUES=/runtime-values
SEEDED_MARKER="${RUNTIME_VALUES}/cnight-supply-minted"

# Inputs produced by the contract-compiler step (it has jq; this container does not).
ICS_ADDR_FILE="${RUNTIME_VALUES}/ics_forever.addr"
RESERVE_ADDR_FILE="${RUNTIME_VALUES}/reserve_forever.addr"
CNIGHT_PLUTUS="${RUNTIME_VALUES}/cnight_policy.plutus"

# cNIGHT amounts in STARS (1 NIGHT = 1_000_000 STARS). These mirror the committed
# local-env Midnight genesis pools (`toolkit show-night-pools`), so the Part B
# monitor's genesis quiescence assertion (C.* == M.*) is the canary that keeps
# these in sync. Total minted = S = 24,000,000,000 NIGHT.
RESERVE_STARS=5000000000873988      # C.R = M.R (reserve_pool)
ICS_STARS=2200000000000000          # C.L = M.U (unlocked = S - M.R - M.L)
FAUCET_STARS=16799999999126012      # C.U = M.L (locked_pool)
TOTAL_MINT_STARS=24000000000000000  # S
# min-UTxO lovelace bundled with each cNIGHT output (matches the e2e/manual scripts).
MIN_UTXO_LOVELACE=1500000

# The bridge is configured to observe this cNIGHT minting policy on every non-mainnet
# environment. We assert the compiled policy matches so a contract change can't
# silently mint under a policy the bridge ignores.
EXPECTED_POLICY_ID=d2dbff622e509dda256fedbd31ef6e9fd98ed49ad91d5c0e07f68af1

if [ -f "$SEEDED_MARKER" ]; then
  echo "cNIGHT already seeded ($SEEDED_MARKER present); skipping."
  exit 0
fi

echo "=== cNIGHT genesis seeding ==="

# Wait for the contract-compiler artifacts (addresses + compiled policy).
for i in {1..60}; do
  if [ -s "$ICS_ADDR_FILE" ] && [ -s "$RESERVE_ADDR_FILE" ] && [ -s "$CNIGHT_PLUTUS" ]; then
    break
  fi
  echo "Waiting for contract-compiler artifacts (attempt $i/60)..."
  sleep 2
done
[ -s "$ICS_ADDR_FILE" ] || { echo "ERROR: $ICS_ADDR_FILE missing"; exit 1; }
[ -s "$RESERVE_ADDR_FILE" ] || { echo "ERROR: $RESERVE_ADDR_FILE missing"; exit 1; }
[ -s "$CNIGHT_PLUTUS" ] || { echo "ERROR: $CNIGHT_PLUTUS missing"; exit 1; }

ICS_ADDR=$(cat "$ICS_ADDR_FILE")
RESERVE_ADDR=$(cat "$RESERVE_ADDR_FILE")
echo "ICS Forever address:     $ICS_ADDR"
echo "Reserve Forever address: $RESERVE_ADDR"

# Verify the compiled cNIGHT policy is the one the bridge observes.
POLICY_ID=$(cardano-cli latest transaction policyid --script-file "$CNIGHT_PLUTUS")
echo "cNIGHT policy id: $POLICY_ID"
if [ "$POLICY_ID" != "$EXPECTED_POLICY_ID" ]; then
  echo "ERROR: compiled cNIGHT policy id $POLICY_ID != expected $EXPECTED_POLICY_ID"
  echo "       (the bridge would not observe cNIGHT under a different policy)"
  exit 1
fi

# The faucet / circulating address is the funded address shared with the e2e suite.
FAUCET_ADDR=$(cardano-cli latest address build \
  --payment-verification-key-file /keys/funded_address.vkey \
  --testnet-magic "$NETWORK_MAGIC")
echo "Faucet (circulating) address: $FAUCET_ADDR"

# Mint the full cNIGHT supply and split it across the three pools in one tx. Reserve/ICS
# are script addresses, so their outputs carry an inline unit datum (matching
# tests/e2e/src/api/cardano.rs::make_bridge_transfer); no bridge metadata (label 6500973),
# so these are not transfers.
#
# The contract-compiler's deploy txs chain through this funded address and can still be
# settling on the node when we query (container exit / kupo confirmation don't guarantee
# the node-socket UTxO set is quiescent), so a freshly-queried UTxO may be spent by the
# next deploy tx before we submit ("All inputs are spent"). Build/sign/submit in a retry
# loop that re-queries fresh pure-ADA UTxOs each attempt.
SEED_TX_ID=""
for attempt in {1..15}; do
  cardano-cli latest query utxo --testnet-magic "$NETWORK_MAGIC" \
    --address "$FAUCET_ADDR" --output-text > /tmp/faucet_utxos.txt || true
  # Pick the two largest pure-ADA UTxOs: "<hash> <ix> <n> lovelace + TxOutDatumNone" (NF==6).
  read -r TX_IN COLLATERAL < <(/busybox awk '
    NR>2 && $4=="lovelace" && $6=="TxOutDatumNone" {
      v=$3+0; ref=$1"#"$2;
      if (v>m1) { m2=m1; u2=u1; m1=v; u1=ref }
      else if (v>m2) { m2=v; u2=ref }
    }
    END { print u1, u2 }' /tmp/faucet_utxos.txt)
  if [ -z "$TX_IN" ] || [ -z "$COLLATERAL" ]; then
    echo "No two pure-ADA UTxOs at faucet yet (attempt $attempt/15); waiting..."
    sleep 4
    continue
  fi
  echo "Attempt $attempt/15: minting $TOTAL_MINT_STARS cNIGHT (R=$RESERVE_STARS / L=$ICS_STARS / U=$FAUCET_STARS)"
  echo "  funding=$TX_IN collateral=$COLLATERAL"

  if ! cardano-cli latest transaction build \
      --testnet-magic "$NETWORK_MAGIC" \
      --tx-in "$TX_IN" \
      --tx-in-collateral "$COLLATERAL" \
      --tx-out "$RESERVE_ADDR+$MIN_UTXO_LOVELACE + $RESERVE_STARS $POLICY_ID" \
      --tx-out-inline-datum-value '{"constructor": 0, "fields": []}' \
      --tx-out "$ICS_ADDR+$MIN_UTXO_LOVELACE + $ICS_STARS $POLICY_ID" \
      --tx-out-inline-datum-value '{"constructor": 0, "fields": []}' \
      --tx-out "$FAUCET_ADDR+$MIN_UTXO_LOVELACE + $FAUCET_STARS $POLICY_ID" \
      --mint "$TOTAL_MINT_STARS $POLICY_ID" \
      --mint-script-file "$CNIGHT_PLUTUS" \
      --mint-redeemer-value "{}" \
      --change-address "$FAUCET_ADDR" \
      --out-file /tmp/cnight-supply.raw; then
    echo "  build failed (stale UTxO?); re-querying after a short wait..."
    sleep 4
    continue
  fi

  cardano-cli latest transaction sign \
    --tx-body-file /tmp/cnight-supply.raw \
    --signing-key-file /keys/funded_address.skey \
    --testnet-magic "$NETWORK_MAGIC" \
    --out-file /tmp/cnight-supply.signed

  # `transaction txid` may print either a bare hash or JSON ({"txhash":"..."});
  # extract the 64-hex id either way.
  txid=$(cardano-cli latest transaction txid --tx-file /tmp/cnight-supply.signed \
    | /busybox grep -oE '[0-9a-f]{64}' | /busybox head -1)
  echo "  submitting tx $txid ..."
  if cardano-cli latest transaction submit \
      --tx-file /tmp/cnight-supply.signed \
      --testnet-magic "$NETWORK_MAGIC"; then
    SEED_TX_ID="$txid"
    break
  fi
  echo "  submit failed (inputs spent / churn); retrying with fresh UTxOs..."
  sleep 4
done
if [ -z "$SEED_TX_ID" ]; then
  echo "ERROR: failed to submit the cNIGHT seeding tx after 15 attempts"
  exit 1
fi

# Require the cNIGHT to land at the ICS address before writing the marker: midnight-setup
# uses the marker as the bridge `initial_data_checkpoint`, which must point at a confirmed
# tx — otherwise the env could start from a checkpoint whose seeded pools don't exist.
echo "Waiting for the seeding tx to be included on-chain..."
included=false
for i in {1..60}; do
  if cardano-cli latest query utxo --testnet-magic "$NETWORK_MAGIC" \
       --address "$ICS_ADDR" --output-text 2>/dev/null \
       | /busybox grep -q "$POLICY_ID"; then
    echo "Seeded ICS cNIGHT confirmed on-chain."
    included=true
    break
  fi
  echo "Waiting for inclusion (attempt $i/60)..."
  sleep 2
done
if [ "$included" != true ]; then
  echo "ERROR: seeding tx $SEED_TX_ID submitted but not confirmed at the ICS address within the budget"
  exit 1
fi

# The marker doubles as the seeding tx hash midnight-setup anchors the bridge checkpoint to.
echo "$SEED_TX_ID" > "$SEEDED_MARKER"
echo "=== cNIGHT genesis seeding complete (tx $SEED_TX_ID) ==="
