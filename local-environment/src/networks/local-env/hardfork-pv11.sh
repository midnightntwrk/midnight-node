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

# hardfork-pv11.sh — Perform a PV10 → PV11 (Dijkstra) hard fork on local-env
#
# Prerequisites:
#   - cardano-node 10.7.0+ (supports PV11/Dijkstra era)
#   - ExperimentalHardForksEnabled: true in cardano node config
#   - DijkstraGenesisFile configured in cardano node config
#   - Key-based CC member in Conway genesis (already configured)
#   - Governance keys in configurations/cardano/governance-keys/ (already generated)
#
# Step-by-step guide:
#   1. Start local-env:
#        npm run run:local-env
#   2. Wait for the environment to be fully ready (~2 minutes)
#   3. Apply the cardano-node 10.7.0 and db-sync image upgrade from the
#      cardano-10.7.0-upgrade branch, then restart with: npm run run:local-env
#   4. Run the hard fork:
#        ./hardfork-pv11.sh run
#   5. The script takes ~5 minutes (5 epochs). It will:
#      - Register a governance stake address, CC hot credentials, and a DRep
#      - Delegate stake to the DRep and pool
#      - Submit a PV11 hard fork governance action
#      - Vote yes from CC, SPO, and DRep
#      - Wait for ratification and enactment
#   6. Verify with:
#        docker exec cardano-node-1 cardano-cli latest query protocol-parameters --testnet-magic 42 | jq '.protocolVersion'
#
# This script also has a "setup" subcommand that was used to generate the
# governance keys and modify the Conway genesis. This has already been done
# and the results are committed — you should not need to run it again.
#   ./hardfork-pv11.sh setup  — (historical) Generate keys + modify Conway genesis

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CONTAINER="cardano-node-1"
TESTNET_MAGIC=42
GOV_DIR="$SCRIPT_DIR/configurations/cardano/governance-keys"
CONWAY_GENESIS="$SCRIPT_DIR/configurations/genesis/conway/genesis.conway.json"
CARDANO_IMAGE="${CARDANO_IMAGE:-ghcr.io/intersectmbo/cardano-node:10.5.2}"

# ─── Helpers ─────────────────────────────────────────────────────────────────

cli() {
  docker exec "$CONTAINER" cardano-cli "$@"
}

query_tip() {
  cli latest query tip --testnet-magic "$TESTNET_MAGIC"
}

get_epoch() {
  query_tip | jq -r '.epoch'
}

wait_for_epoch() {
  local target=$1
  echo "  Waiting for epoch $target..."
  while true; do
    local current
    current=$(get_epoch)
    if [ "$current" -ge "$target" ]; then
      echo "  Reached epoch $current"
      return
    fi
    sleep 2
  done
}

wait_for_next_block() {
  local current_block
  current_block=$(query_tip | jq -r '.block')
  while true; do
    local new_block
    new_block=$(query_tip | jq -r '.block')
    if [ "$new_block" -gt "$current_block" ]; then
      return
    fi
    sleep 1
  done
}

# Pick the first UTXO at an address with at least $min_lovelace
pick_utxo() {
  local addr=$1
  local min_lovelace=${2:-10000000}
  cli latest query utxo --testnet-magic "$TESTNET_MAGIC" --address "$addr" --output-json | \
    jq -r --argjson min "$min_lovelace" \
      'to_entries | map(select(.value.value.lovelace > $min)) | sort_by(-.value.value.lovelace) | .[0].key'
}

# ─── Phase 1: Setup ─────────────────────────────────────────────────────────

do_setup() {
  echo "═══════════════════════════════════════════════════════════"
  echo "  Phase 1: Setup — Generate keys & modify Conway genesis"
  echo "═══════════════════════════════════════════════════════════"
  echo

  if [ -f "$GOV_DIR/cc-cold.skey" ]; then
    echo "Governance keys already exist in $GOV_DIR"
    echo "Delete the directory to regenerate: rm -rf $GOV_DIR"
    echo
    read -rp "Continue with existing keys? [y/N] " yn
    if [[ ! "$yn" =~ ^[Yy] ]]; then
      exit 0
    fi
  fi

  mkdir -p "$GOV_DIR"

  echo "[1/5] Generating CC cold key pair..."
  docker run --rm -v "$GOV_DIR:/keys" --entrypoint cardano-cli "$CARDANO_IMAGE" \
    latest governance committee key-gen-cold \
      --cold-verification-key-file /governance-keys/cc-cold.vkey \
      --cold-signing-key-file /governance-keys/cc-cold.skey

  echo "[2/5] Generating CC hot key pair..."
  docker run --rm -v "$GOV_DIR:/keys" --entrypoint cardano-cli "$CARDANO_IMAGE" \
    latest governance committee key-gen-hot \
      --verification-key-file /governance-keys/cc-hot.vkey \
      --signing-key-file /governance-keys/cc-hot.skey

  echo "[3/5] Generating DRep key pair..."
  docker run --rm -v "$GOV_DIR:/keys" --entrypoint cardano-cli "$CARDANO_IMAGE" \
    latest governance drep key-gen \
      --verification-key-file /governance-keys/drep.vkey \
      --signing-key-file /governance-keys/drep.skey

  echo "[4/5] Generating governance stake key pair..."
  docker run --rm -v "$GOV_DIR:/keys" --entrypoint cardano-cli "$CARDANO_IMAGE" \
    latest stake-address key-gen \
      --verification-key-file /governance-keys/gov-stake.vkey \
      --signing-key-file /governance-keys/gov-stake.skey

  echo "[5/5] Computing key hashes..."
  CC_COLD_HASH=$(docker run --rm -v "$GOV_DIR:/keys" --entrypoint cardano-cli "$CARDANO_IMAGE" \
    latest governance committee key-hash \
      --verification-key-file /governance-keys/cc-cold.vkey)
  echo "  CC cold key hash: $CC_COLD_HASH"
  echo "$CC_COLD_HASH" > "$GOV_DIR/cc-cold.hash"

  DREP_ID=$(docker run --rm -v "$GOV_DIR:/keys" --entrypoint cardano-cli "$CARDANO_IMAGE" \
    latest governance drep id \
      --drep-verification-key-file /governance-keys/drep.vkey \
      --output-hex)
  echo "  DRep ID: $DREP_ID"
  echo "$DREP_ID" > "$GOV_DIR/drep.hash"

  echo
  echo "Modifying Conway genesis..."

  # Add key-based CC member (alongside existing script member) and adjust thresholds
  # Also lower governance deposit for easier local testing
  jq --arg hash "keyHash-$CC_COLD_HASH" '{
      # Add key-based CC member with long term (epoch 10000)
      committee: {
        members: (.committee.members + {($hash): 10000}),
        threshold: {numerator: 1, denominator: 2}
      },
      # Lower governance deposit to 1000 ADA
      govActionDeposit: 1000000000
    } + (. | del(.committee, .govActionDeposit)) |
    # Reconstruct in original key order
    {
      poolVotingThresholds,
      dRepVotingThresholds,
      committeeMinSize,
      committeeMaxTermLength,
      govActionLifetime,
      govActionDeposit,
      dRepDeposit,
      dRepActivity,
      minFeeRefScriptCostPerByte,
      plutusV3CostModel,
      constitution,
      committee
    }' "$CONWAY_GENESIS" > "$CONWAY_GENESIS.tmp" && mv "$CONWAY_GENESIS.tmp" "$CONWAY_GENESIS"

  echo "  Added CC member: keyHash-$CC_COLD_HASH (term until epoch 10000)"
  echo "  Committee threshold: 1/2"
  echo "  Governance deposit: 1,000 ADA"
  echo

  echo "═══════════════════════════════════════════════════════════"
  echo "  Setup complete!"
  echo "═══════════════════════════════════════════════════════════"
  echo
  echo "  Generated keys in: $GOV_DIR"
  echo "  Modified:           $CONWAY_GENESIS"
  echo
  echo "  Next steps:"
  echo "    1. Restart the environment:"
  echo "       docker compose down -v && docker compose up -d"
  echo "    2. Wait ~2 minutes for everything to start"
  echo "    3. Run the hard fork:"
  echo "       ./hardfork-pv11.sh run"
  echo
}

# ─── Phase 2: Execute Hard Fork ─────────────────────────────────────────────

do_run() {
  echo "═══════════════════════════════════════════════════════════"
  echo "  Phase 2: Execute PV10 → PV11 Hard Fork"
  echo "═══════════════════════════════════════════════════════════"
  echo

  # Verify keys exist
  if [ ! -f "$GOV_DIR/cc-cold.skey" ]; then
    echo "ERROR: Governance keys not found. Run './hardfork-pv11.sh setup' first."
    exit 1
  fi

  # Verify current protocol version
  local current_pv
  current_pv=$(cli latest query protocol-parameters --testnet-magic "$TESTNET_MAGIC" | jq '.protocolVersion.major')
  if [ "$current_pv" != "10" ]; then
    echo "ERROR: Expected protocol version 10, got $current_pv"
    exit 1
  fi
  echo "Current protocol version: PV$current_pv (Conway era)"
  echo "Target: PV11"
  echo

  # Verify governance keys are mounted in the container
  echo "Verifying governance keys are mounted..."
  if ! docker exec "$CONTAINER" test -f /governance-keys/cc-cold.skey; then
    echo "ERROR: Governance keys not mounted in container."
    echo "Ensure docker-compose.yml mounts ./configurations/cardano/governance-keys:/keys/governance"
    exit 1
  fi
  echo "  Done."
  echo

  # ── Build addresses ──
  local funded_addr gov_addr gov_stake_addr pool_id
  funded_addr=$(cli latest address build \
    --payment-verification-key-file /keys/funded_address.vkey \
    --testnet-magic "$TESTNET_MAGIC")

  gov_addr=$(cli latest address build \
    --payment-verification-key-file /keys/funded_address.vkey \
    --stake-verification-key-file /governance-keys/gov-stake.vkey \
    --testnet-magic "$TESTNET_MAGIC")

  gov_stake_addr=$(cli latest stake-address build \
    --stake-verification-key-file /governance-keys/gov-stake.vkey \
    --testnet-magic "$TESTNET_MAGIC")

  pool_id=$(cli latest stake-pool id --cold-verification-key-file /keys/cold.vkey)

  echo "Funded address:  $funded_addr"
  echo "Gov base addr:   $gov_addr"
  echo "Gov stake addr:  $gov_stake_addr"
  echo "SPO pool ID:     $pool_id"
  echo

  # ── Step 1: Register stake address + fund gov address ──
  echo "──────────────────────────────────────────────────────────"
  echo "Step 1: Register governance stake address & fund gov addr"
  echo "──────────────────────────────────────────────────────────"

  cli latest stake-address registration-certificate \
    --stake-verification-key-file /governance-keys/gov-stake.vkey \
    --key-reg-deposit-amt 400000 \
    --out-file /data/gov-stake-reg.cert

  local utxo
  utxo=$(pick_utxo "$funded_addr" 5000000000)
  echo "  Using UTXO: $utxo"

  cli latest transaction build \
    --testnet-magic "$TESTNET_MAGIC" \
    --tx-in "$utxo" \
    --tx-out "$gov_addr+5000000000" \
    --certificate-file /data/gov-stake-reg.cert \
    --change-address "$funded_addr" \
    --out-file /data/step1.tx

  cli latest transaction sign \
    --tx-body-file /data/step1.tx \
    --signing-key-file /keys/funded_address.skey \
    --signing-key-file /governance-keys/gov-stake.skey \
    --testnet-magic "$TESTNET_MAGIC" \
    --out-file /data/step1.signed

  cli latest transaction submit \
    --testnet-magic "$TESTNET_MAGIC" \
    --tx-file /data/step1.signed

  echo "  Stake registration + fund submitted."
  wait_for_next_block
  echo

  # ── Step 2: Authorize CC hot credentials ──
  echo "──────────────────────────────────────────────────────────"
  echo "Step 2: Authorize CC hot credentials"
  echo "──────────────────────────────────────────────────────────"

  cli latest governance committee create-hot-key-authorization-certificate \
    --cold-verification-key-file /governance-keys/cc-cold.vkey \
    --hot-verification-key-file /governance-keys/cc-hot.vkey \
    --out-file /data/cc-auth.cert

  utxo=$(pick_utxo "$funded_addr" 5000000)
  echo "  Using UTXO: $utxo"

  cli latest transaction build \
    --testnet-magic "$TESTNET_MAGIC" \
    --tx-in "$utxo" \
    --certificate-file /data/cc-auth.cert \
    --change-address "$funded_addr" \
    --out-file /data/step2.tx

  cli latest transaction sign \
    --tx-body-file /data/step2.tx \
    --signing-key-file /keys/funded_address.skey \
    --signing-key-file /governance-keys/cc-cold.skey \
    --testnet-magic "$TESTNET_MAGIC" \
    --out-file /data/step2.signed

  cli latest transaction submit \
    --testnet-magic "$TESTNET_MAGIC" \
    --tx-file /data/step2.signed

  echo "  CC hot credentials authorized."
  wait_for_next_block
  echo

  # ── Step 3: Register DRep ──
  echo "──────────────────────────────────────────────────────────"
  echo "Step 3: Register DRep"
  echo "──────────────────────────────────────────────────────────"

  local drep_deposit
  drep_deposit=$(cli latest query protocol-parameters --testnet-magic "$TESTNET_MAGIC" | jq -r '.dRepDeposit')
  echo "  DRep deposit: $drep_deposit lovelace"

  cli latest governance drep registration-certificate \
    --drep-verification-key-file /governance-keys/drep.vkey \
    --key-reg-deposit-amt "$drep_deposit" \
    --out-file /data/drep-reg.cert

  utxo=$(pick_utxo "$funded_addr" 1000000000)
  echo "  Using UTXO: $utxo"

  cli latest transaction build \
    --testnet-magic "$TESTNET_MAGIC" \
    --tx-in "$utxo" \
    --certificate-file /data/drep-reg.cert \
    --change-address "$funded_addr" \
    --out-file /data/step3.tx

  cli latest transaction sign \
    --tx-body-file /data/step3.tx \
    --signing-key-file /keys/funded_address.skey \
    --signing-key-file /governance-keys/drep.skey \
    --testnet-magic "$TESTNET_MAGIC" \
    --out-file /data/step3.signed

  cli latest transaction submit \
    --testnet-magic "$TESTNET_MAGIC" \
    --tx-file /data/step3.signed

  echo "  DRep registered."
  wait_for_next_block
  echo

  # ── Step 4: Delegate to DRep + pool ──
  echo "──────────────────────────────────────────────────────────"
  echo "Step 4: Delegate stake to DRep and pool"
  echo "──────────────────────────────────────────────────────────"

  cli latest stake-address vote-delegation-certificate \
    --stake-verification-key-file /governance-keys/gov-stake.vkey \
    --drep-verification-key-file /governance-keys/drep.vkey \
    --out-file /data/vote-deleg.cert

  cli latest stake-address stake-delegation-certificate \
    --stake-verification-key-file /governance-keys/gov-stake.vkey \
    --stake-pool-id "$pool_id" \
    --out-file /data/stake-deleg.cert

  utxo=$(pick_utxo "$gov_addr" 2000000)
  echo "  Using gov UTXO: $utxo"

  cli latest transaction build \
    --testnet-magic "$TESTNET_MAGIC" \
    --tx-in "$utxo" \
    --certificate-file /data/vote-deleg.cert \
    --certificate-file /data/stake-deleg.cert \
    --change-address "$gov_addr" \
    --out-file /data/step4.tx

  cli latest transaction sign \
    --tx-body-file /data/step4.tx \
    --signing-key-file /keys/funded_address.skey \
    --signing-key-file /governance-keys/gov-stake.skey \
    --testnet-magic "$TESTNET_MAGIC" \
    --out-file /data/step4.signed

  cli latest transaction submit \
    --testnet-magic "$TESTNET_MAGIC" \
    --tx-file /data/step4.signed

  echo "  Delegation submitted."
  wait_for_next_block

  # Wait for delegation to take effect at next epoch boundary
  local current_epoch
  current_epoch=$(get_epoch)
  echo "  Waiting for delegation to activate (epoch $((current_epoch + 1)))..."
  wait_for_epoch $((current_epoch + 1))
  echo

  # ── Step 5: Submit PV11 hard fork governance action ──
  echo "──────────────────────────────────────────────────────────"
  echo "Step 5: Submit PV11 hard fork governance action"
  echo "──────────────────────────────────────────────────────────"

  local gov_deposit prev_hf prev_args=""
  gov_deposit=$(cli latest query protocol-parameters --testnet-magic "$TESTNET_MAGIC" | jq -r '.govActionDeposit')
  echo "  Governance action deposit: $gov_deposit lovelace"

  # Check for previous hard fork governance action
  prev_hf=$(cli latest query gov-state --testnet-magic "$TESTNET_MAGIC" | \
    jq -r '.nextRatifyState.nextEnactState.prevGovActionIds.HardFork')

  if [ "$prev_hf" != "null" ] && [ -n "$prev_hf" ]; then
    local prev_tx_id prev_index
    prev_tx_id=$(echo "$prev_hf" | jq -r '.txId')
    prev_index=$(echo "$prev_hf" | jq -r '.govActionIx')
    prev_args="--prev-governance-action-tx-id $prev_tx_id --prev-governance-action-index $prev_index"
    echo "  Previous HF action: $prev_tx_id#$prev_index"
  else
    echo "  No previous hard fork action (first governance HF on this chain)"
  fi

  # shellcheck disable=SC2086
  cli latest governance action create-hardfork \
    --testnet \
    --governance-action-deposit "$gov_deposit" \
    --deposit-return-stake-verification-key-file /governance-keys/gov-stake.vkey \
    --protocol-major-version 11 \
    --protocol-minor-version 0 \
    $prev_args \
    --anchor-url "https://local-env.test/hardfork-pv11" \
    --anchor-data-hash "0000000000000000000000000000000000000000000000000000000000000001" \
    --out-file /data/hardfork-pv11.action

  utxo=$(pick_utxo "$funded_addr" "$((gov_deposit + 5000000))")
  echo "  Using UTXO: $utxo"

  # Use build-raw to avoid anchor URL validation (the node tries to fetch the URL
  # during 'transaction build', which fails on local-env with fake URLs)
  local utxo_value
  utxo_value=$(cli latest query utxo --testnet-magic "$TESTNET_MAGIC" --address "$funded_addr" --output-json | \
    jq -r --arg u "$utxo" '.[$u].value.lovelace')
  local fee=300000
  local change=$((utxo_value - gov_deposit - fee))

  cli latest transaction build-raw \
    --tx-in "$utxo" \
    --tx-out "$funded_addr+$change" \
    --proposal-file /data/hardfork-pv11.action \
    --fee "$fee" \
    --out-file /data/step5.tx

  cli latest transaction sign \
    --tx-body-file /data/step5.tx \
    --signing-key-file /keys/funded_address.skey \
    --signing-key-file /governance-keys/gov-stake.skey \
    --testnet-magic "$TESTNET_MAGIC" \
    --out-file /data/step5.signed

  local hf_tx_id
  hf_tx_id=$(cli latest transaction txid --tx-file /data/step5.signed)
  echo "  Hard fork action TX ID: $hf_tx_id"

  cli latest transaction submit \
    --testnet-magic "$TESTNET_MAGIC" \
    --tx-file /data/step5.signed

  echo "  Hard fork governance action submitted!"
  wait_for_next_block

  # Wait for the proposal to appear in the next epoch (votable)
  current_epoch=$(get_epoch)
  echo "  Waiting for proposal to be votable (epoch $((current_epoch + 1)))..."
  wait_for_epoch $((current_epoch + 1))
  echo

  # ── Step 6: Vote on the hard fork ──
  echo "──────────────────────────────────────────────────────────"
  echo "Step 6: Submit votes (CC + SPO + DRep)"
  echo "──────────────────────────────────────────────────────────"

  # Get the action ID from governance state
  local action_tx_id action_index
  action_tx_id=$(cli latest query gov-state --testnet-magic "$TESTNET_MAGIC" | \
    jq -r '.proposals | map(select(.proposalProcedure.govAction.tag == "HardForkInitiation")) | .[0].actionId.txId')
  action_index=$(cli latest query gov-state --testnet-magic "$TESTNET_MAGIC" | \
    jq -r '.proposals | map(select(.proposalProcedure.govAction.tag == "HardForkInitiation")) | .[0].actionId.govActionIx')

  if [ "$action_tx_id" = "null" ] || [ -z "$action_tx_id" ]; then
    echo "  ERROR: Hard fork proposal not found in governance state!"
    echo "  Checking governance state..."
    cli latest query gov-state --testnet-magic "$TESTNET_MAGIC" | jq '.proposals'
    exit 1
  fi
  echo "  Voting on action: $action_tx_id#$action_index"
  echo

  # ── CC Vote ──
  echo "  [CC] Submitting Constitutional Committee vote..."
  cli latest governance vote create \
    --yes \
    --governance-action-tx-id "$action_tx_id" \
    --governance-action-index "$action_index" \
    --cc-hot-verification-key-file /governance-keys/cc-hot.vkey \
    --out-file /data/cc-vote.vote

  utxo=$(pick_utxo "$funded_addr" 5000000)
  cli latest transaction build \
    --testnet-magic "$TESTNET_MAGIC" \
    --tx-in "$utxo" \
    --vote-file /data/cc-vote.vote \
    --change-address "$funded_addr" \
    --out-file /data/cc-vote.tx

  cli latest transaction sign \
    --tx-body-file /data/cc-vote.tx \
    --signing-key-file /keys/funded_address.skey \
    --signing-key-file /governance-keys/cc-hot.skey \
    --testnet-magic "$TESTNET_MAGIC" \
    --out-file /data/cc-vote.signed

  cli latest transaction submit \
    --testnet-magic "$TESTNET_MAGIC" \
    --tx-file /data/cc-vote.signed
  echo "  [CC] Vote submitted."
  wait_for_next_block

  # ── SPO Vote ──
  echo "  [SPO] Submitting Stake Pool Operator vote..."
  cli latest governance vote create \
    --yes \
    --governance-action-tx-id "$action_tx_id" \
    --governance-action-index "$action_index" \
    --cold-verification-key-file /keys/cold.vkey \
    --out-file /data/spo-vote.vote

  utxo=$(pick_utxo "$funded_addr" 5000000)
  cli latest transaction build \
    --testnet-magic "$TESTNET_MAGIC" \
    --tx-in "$utxo" \
    --vote-file /data/spo-vote.vote \
    --change-address "$funded_addr" \
    --out-file /data/spo-vote.tx

  cli latest transaction sign \
    --tx-body-file /data/spo-vote.tx \
    --signing-key-file /keys/funded_address.skey \
    --signing-key-file /keys/cold.skey \
    --testnet-magic "$TESTNET_MAGIC" \
    --out-file /data/spo-vote.signed

  cli latest transaction submit \
    --testnet-magic "$TESTNET_MAGIC" \
    --tx-file /data/spo-vote.signed
  echo "  [SPO] Vote submitted."
  wait_for_next_block

  # ── DRep Vote ──
  echo "  [DRep] Submitting DRep vote..."
  cli latest governance vote create \
    --yes \
    --governance-action-tx-id "$action_tx_id" \
    --governance-action-index "$action_index" \
    --drep-verification-key-file /governance-keys/drep.vkey \
    --out-file /data/drep-vote.vote

  utxo=$(pick_utxo "$funded_addr" 5000000)
  cli latest transaction build \
    --testnet-magic "$TESTNET_MAGIC" \
    --tx-in "$utxo" \
    --vote-file /data/drep-vote.vote \
    --change-address "$funded_addr" \
    --out-file /data/drep-vote.tx

  cli latest transaction sign \
    --tx-body-file /data/drep-vote.tx \
    --signing-key-file /keys/funded_address.skey \
    --signing-key-file /governance-keys/drep.skey \
    --testnet-magic "$TESTNET_MAGIC" \
    --out-file /data/drep-vote.signed

  cli latest transaction submit \
    --testnet-magic "$TESTNET_MAGIC" \
    --tx-file /data/drep-vote.signed
  echo "  [DRep] Vote submitted."
  wait_for_next_block
  echo

  # ── Step 7: Wait for ratification & enactment ──
  echo "──────────────────────────────────────────────────────────"
  echo "Step 7: Wait for ratification & enactment"
  echo "──────────────────────────────────────────────────────────"

  current_epoch=$(get_epoch)
  echo "  Current epoch: $current_epoch"
  echo "  Epochs are ~60 seconds each on local-env."

  echo "  Waiting for epoch $((current_epoch + 1)) (ratification)..."
  wait_for_epoch $((current_epoch + 1))

  # Check if ratified
  local ratified_hf
  ratified_hf=$(cli latest query gov-state --testnet-magic "$TESTNET_MAGIC" | \
    jq -r '.nextRatifyState.nextEnactState.prevGovActionIds.HardFork')
  if [ "$ratified_hf" != "null" ] && [ -n "$ratified_hf" ]; then
    echo "  Hard fork action ratified!"
  else
    echo "  Not yet ratified. Waiting one more epoch..."
    wait_for_epoch $((current_epoch + 2))
  fi

  echo "  Waiting for enactment..."
  current_epoch=$(get_epoch)
  wait_for_epoch $((current_epoch + 1))
  echo

  # ── Verification ──
  echo "══════════════════════════════════════════════════════════"
  echo "  Verification"
  echo "══════════════════════════════════════════════════════════"

  local new_pv
  new_pv=$(cli latest query protocol-parameters --testnet-magic "$TESTNET_MAGIC" | jq '.protocolVersion.major')

  echo
  query_tip
  echo

  if [ "$new_pv" = "11" ]; then
    echo "  ✓ SUCCESS! Protocol version is now PV$new_pv"
    echo "  Hard fork from PV10 → PV11 completed successfully!"
  else
    echo "  ✗ Protocol version is PV$new_pv (expected 11)"
    echo
    echo "  The hard fork may need more time. Check governance state:"
    echo "    docker exec $CONTAINER cardano-cli latest query gov-state --testnet-magic $TESTNET_MAGIC | jq '.nextRatifyState'"
    echo
    echo "  Common issues:"
    echo "  - DRep delegation not yet active (wait another epoch)"
    echo "  - SPO stake not meeting 51% threshold"
    echo "  - Governance action expired (lifetime: 30 epochs)"
    echo
    echo "  You can re-run this script to retry."
  fi
  echo
}

# ─── Main ────────────────────────────────────────────────────────────────────

case "${1:-}" in
  setup)
    do_setup
    ;;
  run)
    do_run
    ;;
  *)
    echo "Usage: $0 {setup|run}"
    echo
    echo "  setup  — Generate governance keys + modify Conway genesis (requires restart)"
    echo "  run    — Execute the PV10 → PV11 hard fork on the running network"
    echo
    echo "Full workflow:"
    echo "  1. $0 setup"
    echo "  2. docker compose down -v && docker compose up -d"
    echo "  3. Wait ~2 minutes for services to start"
    echo "  4. $0 run"
    exit 1
    ;;
esac
