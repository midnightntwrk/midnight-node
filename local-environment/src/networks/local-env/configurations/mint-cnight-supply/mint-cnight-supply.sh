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

# Seed the Cardano side of the cNIGHT -> mNIGHT bridge so the cross-chain pool
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

# Inputs produced by the contract-compiler step. Everything needed is derived from
# these two with jq (which the cardano-node image ships), same as midnight-setup does.
CONTRACTS_INFO="${RUNTIME_VALUES}/contracts-info.json"
PLUTUS_JSON="${RUNTIME_VALUES}/plutus-local.json"
CNIGHT_PLUTUS=/tmp/cnight_policy.plutus

# cNIGHT amounts in STARS (1 NIGHT = 1_000_000 STARS). These mirror the committed
# local-env Midnight genesis pool inputs (res/local/{reserve,ics}-config.json →
# `toolkit show-night-pools`), so the Part B monitor's genesis quiescence assertion
# (C.* == M.*) is the canary that keeps these in sync. Total minted = S =
# 24,000,000,000 NIGHT. The ICS seed is only the 1.2B treasury baseline; the faucet
# bridge transfer below moves a further 1B faucet→ICS as a normal observed transfer.
RESERVE_STARS=5000000000873988      # C.R = M.R (reserve_pool)
ICS_STARS=1200000000000000          # C.L = treasury baseline (ics-config total_amount)
FAUCET_STARS=17799999999126012      # C.U = S - C.R - C.L (circulating; funds the bridge transfer)
TOTAL_MINT_STARS=24000000000000000  # S
MIN_UTXO_LOVELACE=1500000

if [ -f "$SEEDED_MARKER" ]; then
  echo "cNIGHT already seeded ($SEEDED_MARKER present); skipping."
  exit 0
fi

echo "=== cNIGHT genesis seeding ==="
[ -s "$CONTRACTS_INFO" ] || { echo "ERROR: $CONTRACTS_INFO missing"; exit 1; }
[ -s "$PLUTUS_JSON" ] || { echo "ERROR: $PLUTUS_JSON missing"; exit 1; }

ICS_ADDR=$(jq -r '.[] | select(.name == "ICS Forever") | .address' "$CONTRACTS_INFO")
RESERVE_ADDR=$(jq -r '.[] | select(.name == "Reserve Forever") | .address' "$CONTRACTS_INFO")
[ -n "$ICS_ADDR" ] || { echo "ERROR: ICS Forever address missing from $CONTRACTS_INFO"; exit 1; }
[ -n "$RESERVE_ADDR" ] || { echo "ERROR: Reserve Forever address missing from $CONTRACTS_INFO"; exit 1; }
echo "ICS Forever address:     $ICS_ADDR"
echo "Reserve Forever address: $RESERVE_ADDR"

# Wrap the compiled infinite-mint cNIGHT policy into a .plutus text envelope for
# cardano-cli's --mint-script-file.
jq '{type: "PlutusScriptV3", description: "", cborHex: (.validators[] | select(.title == "test_cnight_no_audit.tcnight_mint_infinite.else") | .compiledCode)}' \
  "$PLUTUS_JSON" > "$CNIGHT_PLUTUS"

POLICY_ID=$(jq -r '.validators[] | select(.title == "test_cnight_no_audit.tcnight_mint_infinite.else") | .hash' "$PLUTUS_JSON")
[ -n "$POLICY_ID" ] && [ "$POLICY_ID" != "null" ] || { echo "ERROR: cNIGHT minting policy id not found in $PLUTUS_JSON"; exit 1; }
echo "cNIGHT policy id: $POLICY_ID"

# The faucet / circulating address is the funded address shared with the e2e suite.
FAUCET_ADDR=$(cardano-cli latest address build \
  --payment-verification-key-file /keys/funded_address.vkey \
  --testnet-magic "$NETWORK_MAGIC")
echo "Faucet (circulating) address: $FAUCET_ADDR"

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

# --- Faucet bridge transfer (funds the Midnight dev wallet 0x..01) ---------------------
#
# Send the c2m bridge transfer that funds the dev faucet wallet: lock 1B NIGHT of the
# just-seeded circulating cNIGHT to the ICS validator with the bridge metadata naming the
# wallet (mirrors scripts/cnight-generates-dust/lock_to_ics.sh and
# tests/e2e/src/api/cardano.rs::make_bridge_transfer). Because it spends the seeding tx's
# outputs it lands strictly AFTER the bridge `initial_data_checkpoint` (the seeding tx),
# so the observation processes it as a user transfer; midnight-setup pre-approves its tx
# hash in the c2m-bridge genesis config (`approved_txs`), so claiming it needs no
# governance round. The claim + DUST registration happen post-genesis (init-mnight-faucet).

# 32-byte Midnight recipient: `UnshieldedWallet::default(0x..01).user_address`.
# Regenerate with: midnight-node-toolkit show-address --network local --seed 00..01
#                  | jq -r .userAddress
FAUCET_RECIPIENT_HEX=bc610dd07c52f59012a88c2f9f1c5f34cbacc75b868202975d6f19beaf37284b
# 1B NIGHT = 1e15 STARS: comfortably above the bridge minimum, a small fraction of C.U.
FAUCET_TRANSFER_STARS=1000000000000000
METADATUM_LABEL=6500973
BRIDGE_TX_HASH_FILE="${RUNTIME_VALUES}/faucet-bridge-tx-hash"

# The seeding tx's outputs are deterministic: 0=reserve, 1=ICS, 2=faucet cNIGHT,
# 3=ADA change (appended by `transaction build`) — spend #2 + #3 directly.
echo "=== faucet bridge transfer (recipient $FAUCET_RECIPIENT_HEX) ==="
cat > /tmp/faucet-bridge-metadata.json <<EOF
{
  "$METADATUM_LABEL": {
    "list": [
      { "bytes": "$FAUCET_RECIPIENT_HEX" }
    ]
  }
}
EOF
cardano-cli latest transaction build \
  --testnet-magic "$NETWORK_MAGIC" \
  --tx-in "$SEED_TX_ID#2" \
  --tx-in "$SEED_TX_ID#3" \
  --tx-in-collateral "$SEED_TX_ID#3" \
  --tx-out "$ICS_ADDR+$MIN_UTXO_LOVELACE + $FAUCET_TRANSFER_STARS $POLICY_ID" \
  --tx-out-inline-datum-value '{"constructor": 0, "fields": []}' \
  --metadata-json-file /tmp/faucet-bridge-metadata.json \
  --json-metadata-detailed-schema \
  --change-address "$FAUCET_ADDR" \
  --out-file /tmp/faucet-bridge.raw
cardano-cli latest transaction sign \
  --tx-body-file /tmp/faucet-bridge.raw \
  --signing-key-file /keys/funded_address.skey \
  --testnet-magic "$NETWORK_MAGIC" \
  --out-file /tmp/faucet-bridge.signed
BRIDGE_TX_ID=$(cardano-cli latest transaction txid --tx-file /tmp/faucet-bridge.signed \
  | /busybox grep -oE '[0-9a-f]{64}' | /busybox head -1)
cardano-cli latest transaction submit \
  --tx-file /tmp/faucet-bridge.signed \
  --testnet-magic "$NETWORK_MAGIC"
echo "Waiting for the faucet bridge tx to be included on-chain..."
included=false
for i in {1..60}; do
  if cardano-cli latest query utxo --testnet-magic "$NETWORK_MAGIC" \
       --tx-in "$BRIDGE_TX_ID#0" --output-text 2>/dev/null \
       | /busybox grep -q "$BRIDGE_TX_ID"; then
    echo "Faucet bridge tx confirmed on-chain."
    included=true
    break
  fi
  echo "Waiting for inclusion (attempt $i/60)..."
  sleep 2
done
if [ "$included" != true ]; then
  echo "ERROR: faucet bridge tx $BRIDGE_TX_ID submitted but not confirmed within the budget"
  exit 1
fi
# midnight-setup pre-approves this hash in the c2m-bridge genesis config.
echo "$BRIDGE_TX_ID" > "$BRIDGE_TX_HASH_FILE"
echo "=== faucet bridge transfer submitted (tx $BRIDGE_TX_ID) ==="

# The marker doubles as the seeding tx hash midnight-setup anchors the bridge checkpoint
# to; written last so it only marks a fully successful run. A partial run is not re-runnable
# on the same Cardano volumes (the infinite mint policy would double the supply) — recover
# by tearing the env down (wipes volumes + runtime-values) and re-seeding from scratch.
echo "$SEED_TX_ID" > "$SEEDED_MARKER"
echo "=== cNIGHT genesis seeding complete (tx $SEED_TX_ID) ==="
