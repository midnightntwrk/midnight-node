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

# Fail if a command fails. (No -x: the script narrates every patch + prints the
# patched configs itself, so xtrace would just double every line with '+' noise.)
set -euo pipefail

check_json_validity() {
  local file="$1"
  if ! jq -e . "$file" > /dev/null 2>&1; then
    echo "Error: $file is invalid JSON."
    exit 1
  fi
}

# Big banner for the top-level phases of this job.
phase() {
  echo ""
  echo "############################################################"
  echo "##  $1"
  echo "############################################################"
}

# Smaller banner for each config section within a phase.
section() {
  echo ""
  echo "===== $1 ====="
}

# Patch a JSON file IN PLACE (jq can't read and write the same file, hence tmp+mv).
# The /res mount is the repo working tree, so every patched value lands there —
# drift between the static res/local configs and the deployed reality shows up as
# a git diff, easy to review and commit when regenerating genesis.
patch_json() {
  local file="$1"; shift
  jq "$@" "$file" > "$file.tmp"
  mv "$file.tmp" "$file"
}

# Read contracts-active-epoch saved by contract-compiler
contracts_active_epoch=$(cat /runtime-values/contracts-active-epoch)
echo "Contracts will be active at epoch: $contracts_active_epoch"

echo "Using Partner Chains node version:"
./midnight-node --version

export OGMIOS_URL=http://ogmios:$OGMIOS_PORT

CONTRACT_INFO="/runtime-values/contracts-info.json"
COUNCIL_POLICY_ID=$(jq -r '.[] | select(.name == "Council Forever") | .scriptHash' $CONTRACT_INFO)
COUNCIL_SCRIPT_ADDRESS=$(jq -r '.[] | select(.name == "Council Forever") | .address' $CONTRACT_INFO)
TECHAUTH_POLICY_ID=$(jq -r '.[] | select(.name == "Tech Auth Forever") | .scriptHash' $CONTRACT_INFO)
TECHAUTH_SCRIPT_ADDRESS=$(jq -r '.[] | select(.name == "Tech Auth Forever") | .address' $CONTRACT_INFO)
CNIGHT_MAPPING_VALIDATOR_ADDRESS=$(jq -r '.[] | select(.name == "cNIGHT Generates Dust") | .address' $CONTRACT_INFO)
PLUTUS_INFO="/runtime-values/plutus-local.json"
CNIGHT_MINTING_POLICY_ID=$(jq -r '.validators[] | select(.title == "test_cnight_no_audit.tcnight_mint_infinite.else") | .hash' "$PLUTUS_INFO")
ICS_FOREVER_ADDRESS=$(jq -r '.[] | select(.name == "ICS Forever") | .address' $CONTRACT_INFO)
RESERVE_FOREVER_ADDRESS=$(jq -r '.[] | select(.name == "Reserve Forever") | .address' $CONTRACT_INFO)
REGISTERED_CANDIDATES_ADDRESS=$(jq -r '.[] | select(.name == "Registered Candidate") | .address' $CONTRACT_INFO)
PERMISSIONED_CANDIDATES_POLICY_ID=$(jq -r '.[] | select(.name == "Federated Ops Forever") | .scriptHash' $CONTRACT_INFO)

phase "Patching configs"

section "pc-chain-config.json"
echo "Patching pc-chain-config.json with:"
echo "  cardano_addresses.permissioned_candidates_policy_id: $PERMISSIONED_CANDIDATES_POLICY_ID"
echo "  cardano_addresses.committee_candidates_address: $REGISTERED_CANDIDATES_ADDRESS"
patch_json /res/local/pc-chain-config.json \
   --arg policy_id "$PERMISSIONED_CANDIDATES_POLICY_ID" \
   --arg committee_addr "$REGISTERED_CANDIDATES_ADDRESS" \
   '.cardano_addresses.permissioned_candidates_policy_id = $policy_id
   | .cardano_addresses.committee_candidates_address = $committee_addr'
echo "Patched pc-chain-config.json:"
cat /res/local/pc-chain-config.json


section "federated-authority-config.json"
echo "Patching federated-authority-config.json with:"
echo "  council.address: $COUNCIL_SCRIPT_ADDRESS"
echo "  council.policy_id: $COUNCIL_POLICY_ID"
echo "  technical_committee.address: $TECHAUTH_SCRIPT_ADDRESS"
echo "  technical_committee.policy_id: $TECHAUTH_POLICY_ID"
patch_json /res/local/federated-authority-config.json \
   --arg council_addr "$COUNCIL_SCRIPT_ADDRESS" \
   --arg council_policy "$COUNCIL_POLICY_ID" \
   --arg techauth_addr "$TECHAUTH_SCRIPT_ADDRESS" \
   --arg techauth_policy "$TECHAUTH_POLICY_ID" \
   '.council.address = $council_addr | .council.policy_id = $council_policy | .technical_committee.address = $techauth_addr | .technical_committee.policy_id = $techauth_policy'
echo "Patched federated-authority-config.json:"
cat /res/local/federated-authority-config.json


section "system-parameters-config.json"
echo "Using system-parameters-config.json as is:"
cat /res/local/system-parameters-config.json


section "permissioned-candidates-config.json"
echo "Patching permissioned-candidates-config.json with:"
echo "  permissioned_candidates_policy_id: 0x$PERMISSIONED_CANDIDATES_POLICY_ID"
patch_json /res/local/permissioned-candidates-config.json \
   --arg policy_id "$PERMISSIONED_CANDIDATES_POLICY_ID" \
   '.permissioned_candidates_policy_id = ("0x" + $policy_id)'
echo "Patched permissioned-candidates-config.json:"
cat /res/local/permissioned-candidates-config.json


section "registered-candidates-addresses.json"
echo "Patching registered-candidates-addresses.json with:"
echo "  committee_candidates_address: $REGISTERED_CANDIDATES_ADDRESS"
patch_json /res/local/registered-candidates-addresses.json \
   --arg committee_addr "$REGISTERED_CANDIDATES_ADDRESS" \
   '.committee_candidates_address = $committee_addr'
echo "Patched registered-candidates-addresses.json:"
cat /res/local/registered-candidates-addresses.json


section "cnight-config.json"
echo "Patching cnight-config.json with:"
echo "  addresses.mapping_validator_address: $CNIGHT_MAPPING_VALIDATOR_ADDRESS"
echo "  addresses.cnight_policy_id: $CNIGHT_MINTING_POLICY_ID"
patch_json /res/local/cnight-config.json \
  --arg mapping_addr "$CNIGHT_MAPPING_VALIDATOR_ADDRESS" \
  --arg cnight_pid "$CNIGHT_MINTING_POLICY_ID" \
  '.addresses.mapping_validator_address = $mapping_addr
  | .addresses.cnight_policy_id = $cnight_pid
  | .observed_utxos.end = .observed_utxos.start
  | .observed_utxos.utxos = []
  | .mappings = {}
  | .utxo_owners = {}
  | .next_cardano_position = .observed_utxos.start
  | .system_tx = null'
echo "Patched cnight-config.json:"
cat /res/local/cnight-config.json


CNIGHT_POLICY_ID=$(jq -r '.addresses.cnight_policy_id' /res/local/cnight-config.json)
CNIGHT_ASSET_NAME=$(jq -r '.addresses.cnight_asset_name' /res/local/cnight-config.json)
cnight_seed_tx=$(cat /runtime-values/cnight-supply-minted 2>/dev/null || echo "")

# The seed tx's cNIGHT outputs at <address>, as IcsUtxo/ReserveUtxo JSON objects.
# With no seed marker the filter matches nothing -> empty baseline (old behaviour).
seeded_utxos() {
  curl -s -H 'Content-Type: application/json' \
    -d '{"jsonrpc": "2.0", "method": "queryLedgerState/utxo", "params": {"addresses": ["'"$1"'"]}, "id": 1}' \
    "$OGMIOS_URL" \
  | jq --arg tx "$cnight_seed_tx" --arg pid "$CNIGHT_POLICY_ID" --arg name "$CNIGHT_ASSET_NAME" \
      '[.result[]
        | select(.transaction.id == $tx)
        | {tx_hash: .transaction.id, output_index: .index, amount: .value[$pid][$name]}
        | select(.amount != null)]'
}
ics_utxos=$(seeded_utxos "$ICS_FOREVER_ADDRESS")
reserve_utxos=$(seeded_utxos "$RESERVE_FOREVER_ADDRESS")

section "ics-config.json"
echo "Patching ics-config.json with:"
echo "  illiquid_circulation_supply_validator_address: $ICS_FOREVER_ADDRESS"
echo "  asset.policy_id: $CNIGHT_POLICY_ID"
echo "  utxos/total_amount: cNIGHT outputs of seed tx ${cnight_seed_tx:-<none>}"
patch_json /res/local/ics-config.json \
   --arg addr "$ICS_FOREVER_ADDRESS" --arg pid "$CNIGHT_POLICY_ID" --arg name "$CNIGHT_ASSET_NAME" --argjson utxos "$ics_utxos" \
   '.illiquid_circulation_supply_validator_address = $addr
   | .asset = {policy_id: $pid, asset_name: $name}
   | .utxos = $utxos
   | .total_amount = ($utxos | map(.amount) | add // 0)'
echo "Patched ics-config.json:"
cat /res/local/ics-config.json


section "reserve-config.json"
echo "Patching reserve-config.json with:"
echo "  reserve_validator_address: $RESERVE_FOREVER_ADDRESS"
echo "  asset.policy_id: $CNIGHT_POLICY_ID"
echo "  utxos/total_amount: cNIGHT outputs of seed tx ${cnight_seed_tx:-<none>}"
patch_json /res/local/reserve-config.json \
   --arg addr "$RESERVE_FOREVER_ADDRESS" --arg pid "$CNIGHT_POLICY_ID" --arg name "$CNIGHT_ASSET_NAME" --argjson utxos "$reserve_utxos" \
   '.reserve_validator_address = $addr
   | .asset = {policy_id: $pid, asset_name: $name}
   | .utxos = $utxos
   | .total_amount = ($utxos | map(.amount) | add // 0)'
echo "Patched reserve-config.json:"
cat /res/local/reserve-config.json


# The bridge observes strictly AFTER initial_data_checkpoint, so anchor it to the cNIGHT
# seeding tx (midnight-node#1778): the pre-seeded ICS supply is already reflected in the
# genesis pools, so re-observing it would double-account it. Fall back to the latest UTxO
# tx if the seeding step did not run.
CNIGHT_SEED_MARKER=/runtime-values/cnight-supply-minted
if [ -s "$CNIGHT_SEED_MARKER" ]; then
  existing_tx_hash=$(cat "$CNIGHT_SEED_MARKER")
else
  echo "cNIGHT seed marker absent; falling back to the latest ledger UTxO tx as initial_data_checkpoint"
  existing_tx_hash=$(curl -s -H 'Content-Type: application/json' \
    -d '{"jsonrpc": "2.0", "method": "queryLedgerState/utxo", "id":1}' \
    http://ogmios:1337 | jq -r .result[0].transaction.id)
fi
# Pre-approve the faucet bridge transfer (submitted by mint-cnight-supply strictly after
# the checkpoint tx above) so wallet 0x..01 can claim it without a governance round.
FAUCET_BRIDGE_TX_FILE=/runtime-values/faucet-bridge-tx-hash
approved_txs="[]"
if [ -s "$FAUCET_BRIDGE_TX_FILE" ]; then
  faucet_bridge_tx=$(cat "$FAUCET_BRIDGE_TX_FILE")
  approved_txs="[\"$faucet_bridge_tx\"]"
else
  echo "No faucet bridge tx marker; genesis approved_txs stays empty"
fi

section "c2m-bridge-config.json"
echo "Patching c2m-bridge-config.json with:"
echo "  initial_data_checkpoint: $existing_tx_hash"
echo "  approved_txs: $approved_txs"
patch_json /res/local/c2m-bridge-config.json \
  --arg checkpoint "$existing_tx_hash" \
  --argjson approved "$approved_txs" \
  '.initial_data_checkpoint = $checkpoint | .approved_txs = $approved'
echo "Patched c2m-bridge-config.json:"
cat /res/local/c2m-bridge-config.json

phase "Building chain-spec"

# All chainspec inputs come from the `local` cfg preset (res/cfg/local.toml): the
# chainspec_* paths there are relative (res/local/..., res/genesis/...) and the image
# workdir is /, so they resolve through the /res repo mount — build-spec reads the
# configs patched above, and a locally regenerated genesis takes effect on the next
# bring-up without a node-image rebuild.
export CFG_PRESET=local

./midnight-node build-spec --disable-default-bootnode > chain-spec.json
echo "chain-spec.json file generated."

echo "Amending the chain spec..."
echo "Configuring Epoch Length..."
jq '.genesis.runtimeGenesis.config.sidechain.slotsPerEpoch = 5' chain-spec.json > tmp.json && mv tmp.json chain-spec.json

check_json_validity chain-spec.json

echo "Final chain spec"

echo "Copying chain-spec.json file to /shared/chain-spec.json..."
cp chain-spec.json /shared/chain-spec.json
echo "chain-spec.json generation complete."

echo "Partnerchain configuration is complete, and will be able to start after two mainchain epochs."

phase "Awaiting activation"

echo "Waiting for contracts to become active at epoch $contracts_active_epoch..."
epoch=$(curl -s --request POST \
    --url "http://ogmios:1337" \
    --header 'Content-Type: application/json' \
    --data '{"jsonrpc": "2.0", "method": "queryLedgerState/epoch"}' | jq .result)
echo "Current epoch: $epoch"
while [ "$epoch" -lt "$contracts_active_epoch" ]; do
  sleep 10
  epoch=$(curl -s --request POST \
    --url "http://ogmios:1337" \
    --header 'Content-Type: application/json' \
    --data '{"jsonrpc": "2.0", "method": "queryLedgerState/epoch"}' | jq .result)
  echo "Current epoch: $epoch"
done
echo "DParam is now active!"
