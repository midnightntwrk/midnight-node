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

chmod 600 /keys/*
chmod +x /busybox
chmod 777 /shared

# Removed: this caused permissions errors on host when running tests locally
# chown -R $(id -u):$(id -g) /shared /runtime-values /keys /data

echo "Calculating target time for synchronised chain start..."

# Local env Partner Chains epochs are 30 seconds long. PC and MC epochs have to align. The following line makes MC epoch 0 start at some PC epoch start.
target_time=$(( ($(date +%s) / 30 + 1) * 30 ))
echo "$target_time" > /shared/cardano.start
byron_startTime=$target_time
shelley_systemStart=$(date --utc +"%Y-%m-%dT%H:%M:%SZ" --date="@$target_time")

/busybox sed "s/\"startTime\": [0-9]*/\"startTime\": $byron_startTime/" /shared/byron/genesis.json.base > /shared/byron/genesis.json
echo "Updated startTime value in Byron genesis.json to: $byron_startTime"

/busybox sed "s/\"systemStart\": \"[^\"]*\"/\"systemStart\": \"$shelley_systemStart\"/" /shared/shelley/genesis.json.base > /shared/shelley/genesis.json
echo "Updated systemStart value in Shelley genesis.json to: $shelley_systemStart"

extract_value() {
    local key=$1
    /busybox awk -F':|,' '/"'$key'"/ {print $2}' /shared/shelley/genesis.json.base
}

echo "Parsing vars from Shelley genesis.json..."
mc_epoch_length=$(extract_value "epochLength")
mc_slot_length=$(extract_value "slotLength")
mc_security_param=$(extract_value "securityParam")
mc_active_slots_coeff=$(extract_value "activeSlotsCoeff")

cp /shared/conway/genesis.conway.json.base /shared/conway/genesis.conway.json
cp /shared/shelley/genesis.alonzo.json.base /shared/shelley/genesis.alonzo.json
echo "Created /shared/conway/genesis.conway.json and /shared/shelley/genesis.alonzo.json"

byron_hash=$(/bin/cardano-cli byron genesis print-genesis-hash --genesis-json /shared/byron/genesis.json)
shelley_hash=$(/bin/cardano-cli latest genesis hash --genesis /shared/shelley/genesis.json)
alonzo_hash=$(/bin/cardano-cli latest genesis hash --genesis /shared/shelley/genesis.alonzo.json)
conway_hash=$(/bin/cardano-cli latest genesis hash --genesis /shared/conway/genesis.conway.json)

/busybox sed "s/\"ByronGenesisHash\": \"[^\"]*\"/\"ByronGenesisHash\": \"$byron_hash\"/" /shared/node-1-config.json.base > /shared/node-1-config.json.base.byron
/busybox sed "s/\"ByronGenesisHash\": \"[^\"]*\"/\"ByronGenesisHash\": \"$byron_hash\"/" /shared/db-sync-config.json.base > /shared/db-sync-config.json.base.byron
/busybox sed "s/\"ShelleyGenesisHash\": \"[^\"]*\"/\"ShelleyGenesisHash\": \"$shelley_hash\"/" /shared/node-1-config.json.base.byron > /shared/node-1-config.base.shelley
/busybox sed "s/\"ShelleyGenesisHash\": \"[^\"]*\"/\"ShelleyGenesisHash\": \"$shelley_hash\"/" /shared/db-sync-config.json.base.byron > /shared/db-sync-config.base.shelley
/busybox sed "s/\"AlonzoGenesisHash\": \"[^\"]*\"/\"AlonzoGenesisHash\": \"$alonzo_hash\"/" /shared/node-1-config.base.shelley > /shared/node-1-config.json.base.conway
/busybox sed "s/\"AlonzoGenesisHash\": \"[^\"]*\"/\"AlonzoGenesisHash\": \"$alonzo_hash\"/" /shared/db-sync-config.base.shelley > /shared/db-sync-config.json.base.conway
/busybox sed "s/\"ConwayGenesisHash\": \"[^\"]*\"/\"ConwayGenesisHash\": \"$conway_hash\"/" /shared/node-1-config.json.base.conway > /shared/node-1-config.json
/busybox sed "s/\"ConwayGenesisHash\": \"[^\"]*\"/\"ConwayGenesisHash\": \"$conway_hash\"/" /shared/db-sync-config.json.base.conway > /shared/db-sync-config.json

echo "Updated ByronGenesisHash value in config files to: $byron_hash"
echo "Updated ShelleyGenesisHash value in config files to: $shelley_hash"
echo "Updated ConwayGenesisHash value in config files to: $conway_hash"

MC_ENV_FILE="/tmp/mc.env"
touch "$MC_ENV_FILE"

# Function to add variable to env file
add_env_var() {
    local var_name="$1"
    local value="$2"

    if [ -n "$value" ]; then
        echo "export $var_name=\"$value\"" >> "$MC_ENV_FILE"
        echo "✓ $var_name=$value"
    fi
}

byron_startTimeMillis=$(($byron_startTime * 1000))

# Extract values needed for epoch duration calculation
epoch_duration_millis=$((mc_epoch_length * mc_slot_length * 1000))
slot_duration_millis=$((mc_slot_length * 1000))

add_env_var "CARDANO_SECURITY_PARAMETER" $mc_security_param
add_env_var "CARDANO_ACTIVE_SLOTS_COEFF" $mc_active_slots_coeff
add_env_var "BLOCK_STABILITY_MARGIN" "0"
add_env_var "MC__FIRST_EPOCH_TIMESTAMP_MILLIS" "$byron_startTimeMillis"
add_env_var "MC__FIRST_EPOCH_NUMBER" "0"
add_env_var "MC__EPOCH_DURATION_MILLIS" "$epoch_duration_millis"
add_env_var "MC__FIRST_SLOT_NUMBER" "0"
add_env_var "MC__SLOT_DURATION_MILLIS" "$slot_duration_millis"

cp "$MC_ENV_FILE" /shared/mc.env
cp "$MC_ENV_FILE" /runtime-values/mc.env
echo "Created /shared/mc.env with mainchain env-vars"

echo "Current time is now: $(date +"%H:%M:%S.%3N"). Starting node..."

cardano-node run \
  --topology /shared/node-1-topology.json \
  --database-path /data/db \
  --socket-path /data/node.socket \
  --host-addr 0.0.0.0 \
  --port 32000 \
  --config /shared/node-1-config.json \
  --shelley-kes-key /keys/kes.skey \
  --shelley-vrf-key /keys/vrf.skey \
  --shelley-operational-certificate /keys/node.cert \
  > /data/node.log 2>&1 &
NODE_PID=$!

set +x
echo "Waiting for node.socket..."

for i in {1..60}; do
  if [ -S /data/node.socket ]; then
    echo "Node socket is available."
    break
  fi

  if ! kill -0 $NODE_PID 2>/dev/null; then
    echo "cardano-node process has exited unexpectedly. Dumping logs:"
    cat /data/node.log
    exit 1
  fi

  sleep 1
done

if [ ! -S /data/node.socket ]; then
  echo "Timed out waiting for /data/node.socket"
  cat /data/node.log
  exit 1
fi
set -x

echo "Preparing native token owned by 'funded_address.skey'"
# Policy requires that mints are signed by the funded_address.skey (key hash is e8c300330fe315531ca89d4a2e7d0c80211bc70b473b1ed4979dff2b)
reward_token_policy_id=$(cardano-cli latest transaction policyid --script-file ./shared/reward_token_policy.script)
# hex of "Reward token"
reward_token_asset_name="52657761726420746f6b656e"
echo "Generating new address and funding it with 2x1000 Ada and 10 Ada + 1000000 reward token ($reward_token_policy_id.$reward_token_asset_name)"

new_address=$(cardano-cli latest address build \
  --payment-verification-key-file /keys/funded_address.vkey \
  --testnet-magic 42)

echo "New address created: $new_address"

dave_address="addr_test1vphpcf32drhhznv6rqmrmgpuwq06kug0lkg22ux777rtlqst2er0r"
eve_address="addr_test1vzzt5pwz3pum9xdgxalxyy52m3aqur0n43pcl727l37ggscl8h7v8"
# An address that will keep an UTXO with script of a test V-function, related to the SPO rewards. See v-function.script file.
vfunction_address="addr_test1vzuasm5nqzh7n909f7wang7apjprpg29l2f9sk6shlt84rqep6nyc"

# Define the UTXO details and amounts
tx_in1="781cb948a37c7c38b43872af9b1e22135a94826eafd3740260a6db0a303885d8#0"
tx_in_amount=29993040000000000

# Define output amounts
tx_out1=1000000000 # new_address utxo 1
tx_out2=1000000000 # new_address utxo 2
tx_out3=1000000000 # partner-chains-node-4 (dave)
tx_out4=1000000000 # partner-chains-node-5 (eve)
tx_out5=1000000000 # one-shot-council
tx_out6=1000000000 # one-shot-technical-committee
tx_out5_lovelace=10000000
tx_out5_reward_token="1000000 $reward_token_policy_id.$reward_token_asset_name"
tx_out7=10000000

# Total output without fee
total_output=$((tx_out1 + tx_out2 + tx_out3 + tx_out4 + tx_out5_lovelace + tx_out5 + tx_out6 + tx_out7))

fee=1000000

# Calculate remaining balance to return to the genesis address
change=$((tx_in_amount - total_output - fee))

# Build the raw transaction
cardano-cli latest transaction build-raw \
  --tx-in $tx_in1 \
  --tx-out "$new_address+$tx_out1" \
  --tx-out "$new_address+$tx_out2" \
  --tx-out "$dave_address+$tx_out3" \
  --tx-out "$new_address+$tx_out5" \
  --tx-out "$new_address+$tx_out6" \
  --tx-out "$eve_address+$tx_out4" \
  --tx-out "$new_address+$change" \
  --tx-out "$new_address+$tx_out5_lovelace+$tx_out5_reward_token" \
  --tx-out "$vfunction_address+$tx_out7" \
  --tx-out-reference-script-file /shared/v-function.script \
  --minting-script-file /shared/reward_token_policy.script \
  --mint "$tx_out5_reward_token" \
  --fee $fee \
  --out-file /data/tx.raw

# Sign the transaction
cardano-cli latest transaction sign \
  --tx-body-file /data/tx.raw \
  --signing-key-file /shared/shelley/genesis-utxo.skey \
  --signing-key-file /keys/funded_address.skey \
  --testnet-magic 42 \
  --out-file /data/tx.signed

cat /data/tx.signed

echo "Submitting transaction..."
cardano-cli latest transaction submit \
  --tx-file /data/tx.signed \
  --testnet-magic 42

echo "Transaction submitted to fund registered candidates and governance authority. Waiting 10 seconds for transaction to process..."
sleep 10
echo "Balance:"

# Query UTXOs at new_address, dave_address, and eve_address
echo "Querying UTXO for new_address:"
cardano-cli latest query utxo \
  --testnet-magic 42 \
  --address $new_address

echo "Querying UTXO for Dave address:"
cardano-cli latest query utxo \
  --testnet-magic 42 \
  --address $dave_address

echo "Querying UTXO for Eve address:"
cardano-cli latest query utxo \
  --testnet-magic 42 \
  --address $eve_address

# Save dynamic values to shared config volume for other nodes to use
echo $new_address > /shared/FUNDED_ADDRESS
echo "Created /shared/FUNDED_ADDRESS with value: $new_address"

echo "Querying and saving the first UTXO details for Dave address to /shared/dave.utxo:"
cardano-cli latest query utxo --testnet-magic 42 --address "${dave_address}" | /busybox awk 'NR>2 { print $1 "#" $2; exit }' > /shared/dave.utxo
echo "UTXO details for Dave saved in /shared/dave.utxo."
cat /shared/dave.utxo

echo "Querying and saving the first UTXO details for Eve address to /shared/eve.utxo:"
cardano-cli latest query utxo --testnet-magic 42 --address "${eve_address}" | /busybox awk 'NR>2 { print $1 "#" $2; exit }' > /shared/eve.utxo
echo "UTXO details for Eve saved in /shared/eve.utxo."
cat /shared/eve.utxo

echo "Querying and saving the first UTXO details for new address to /shared/genesis.utxo:"
cardano-cli latest query utxo --testnet-magic 42 --address "${new_address}" | /busybox awk 'NR>2 { print $1 "#" $2; exit }' > /shared/genesis.utxo
cat /shared/genesis.utxo > /runtime-values/genesis.utxo

cat /shared/genesis.utxo

# ============================================================================
# CREATE ONE-SHOT UTXOs FOR GOVERNANCE CONTRACTS
# ============================================================================
echo ""
echo "========================================="
echo "Creating One-Shot UTxOs for Governance"
echo "========================================="

# These UTxOs will be used as one-shot inputs for governance contract minting
# to ensure each governance NFT can only be minted once

# Get available UTxOs for new_address (we'll use different ones for each one-shot)
echo "Querying available UTxOs at $new_address..."
cardano-cli latest query utxo --testnet-magic 42 --address "${new_address}" > /tmp/available_utxos.txt
cat /tmp/available_utxos.txt

# Extract third UTXO for council one-shot (skip first line header, skip second line dashes, get fifth line)
council_input=$(cat /tmp/available_utxos.txt | /busybox awk 'NR==5 { print $1 "#" $2 }')
council_input_amount=$(cat /tmp/available_utxos.txt | /busybox awk 'NR==5 { print $3 }')

# Extract fourth UTXO for council one-shot (skip first line header, skip second line dashes, get sixth line)
techauth_input=$(cat /tmp/available_utxos.txt | /busybox awk 'NR==6 { print $1 "#" $2 }')
techauth_input_amount=$(cat /tmp/available_utxos.txt | /busybox awk 'NR==6 { print $3 }')

echo ""
echo "Creating Council One-Shot UTxO..."
echo "  Input: $council_input"
echo "  Input Amount: $council_input_amount lovelace"

# Council one-shot transaction
# We create a single output with 10 ADA (enough for later use as one-shot)
council_oneshot_amount=10000000
council_fee=200000
council_change=$((council_input_amount - council_oneshot_amount - council_fee))

cardano-cli latest transaction build-raw \
  --tx-in $council_input \
  --tx-out "$new_address+$council_oneshot_amount" \
  --tx-out "$new_address+$council_change" \
  --fee $council_fee \
  --out-file /data/council-oneshot.raw

cardano-cli latest transaction sign \
  --tx-body-file /data/council-oneshot.raw \
  --signing-key-file /keys/funded_address.skey \
  --testnet-magic 42 \
  --out-file /data/council-oneshot.signed

echo "Submitting council one-shot transaction..."
cardano-cli latest transaction submit \
  --tx-file /data/council-oneshot.signed \
  --testnet-magic 42

echo "Waiting 5 seconds for council one-shot transaction to confirm..."
sleep 5

# Query and save the council one-shot UTxO reference
echo "Querying council one-shot UTxO..."
cardano-cli latest query utxo --testnet-magic 42 --address "${new_address}" | /busybox awk -v amount="$council_oneshot_amount" '$3 == amount { print $1 "#" $2; exit }' > /shared/council.oneshot.utxo

council_oneshot_ref=$(cat /shared/council.oneshot.utxo)
council_oneshot_hash=$(echo $council_oneshot_ref | cut -d'#' -f1)
council_oneshot_index=$(echo $council_oneshot_ref | cut -d'#' -f2)

echo "✓ Council One-Shot UTxO Created:"
echo "  Reference: $council_oneshot_ref"
echo "  TX Hash:   $council_oneshot_hash"
echo "  TX Index:  $council_oneshot_index"

# Save to files for contract compilation
echo "$council_oneshot_hash" > /shared/council_oneshot_hash.txt
echo "$council_oneshot_index" > /shared/council_oneshot_index.txt
cp /shared/council.oneshot.utxo /runtime-values/council.oneshot.utxo
cp /shared/council_oneshot_hash.txt /runtime-values/council_oneshot_hash.txt
cp /shared/council_oneshot_index.txt /runtime-values/council_oneshot_index.txt

# Extract fourth UTXO for tech auth one-shot
echo ""
echo "Creating Technical Authority One-Shot UTxO..."
echo "  Input: $techauth_input"
echo "  Input Amount: $techauth_input_amount lovelace"

# Tech auth one-shot transaction
techauth_oneshot_amount=15000000
techauth_fee=200000
techauth_change=$((techauth_input_amount - techauth_oneshot_amount - techauth_fee))

cardano-cli latest transaction build-raw \
  --tx-in $techauth_input \
  --tx-out "$new_address+$techauth_oneshot_amount" \
  --tx-out "$new_address+$techauth_change" \
  --fee $techauth_fee \
  --out-file /data/techauth-oneshot.raw

cardano-cli latest transaction sign \
  --tx-body-file /data/techauth-oneshot.raw \
  --signing-key-file /keys/funded_address.skey \
  --testnet-magic 42 \
  --out-file /data/techauth-oneshot.signed

echo "Submitting tech auth one-shot transaction..."
cardano-cli latest transaction submit \
  --tx-file /data/techauth-oneshot.signed \
  --testnet-magic 42

echo "Waiting 5 seconds for tech auth one-shot transaction to confirm..."
sleep 5

# Query and save the tech auth one-shot UTxO reference
echo "Querying tech auth one-shot UTxO..."
cardano-cli latest query utxo --testnet-magic 42 --address "${new_address}" | /busybox awk -v amount="$techauth_oneshot_amount" '$3 == amount && !seen[$1"#"$2]++ { print $1 "#" $2; exit }' > /shared/techauth.oneshot.utxo

techauth_oneshot_ref=$(cat /shared/techauth.oneshot.utxo)
techauth_oneshot_hash=$(echo $techauth_oneshot_ref | cut -d'#' -f1)
techauth_oneshot_index=$(echo $techauth_oneshot_ref | cut -d'#' -f2)

echo "✓ Technical Authority One-Shot UTxO Created:"
echo "  Reference: $techauth_oneshot_ref"
echo "  TX Hash:   $techauth_oneshot_hash"
echo "  TX Index:  $techauth_oneshot_index"

# Save to files for contract compilation
echo "$techauth_oneshot_hash" > /shared/techauth_oneshot_hash.txt
echo "$techauth_oneshot_index" > /shared/techauth_oneshot_index.txt
cp /shared/techauth.oneshot.utxo /runtime-values/techauth.oneshot.utxo
cp /shared/techauth_oneshot_hash.txt /runtime-values/techauth_oneshot_hash.txt
cp /shared/techauth_oneshot_index.txt /runtime-values/techauth_oneshot_index.txt

echo "Fixing permissions for generated files..."
chown $(id -u):$(id -g) \
  /runtime-values/genesis.utxo \
  /runtime-values/mc.env \
  /shared/genesis.utxo \
  /shared/council.oneshot.utxo \
  /shared/council_oneshot_hash.txt \
  /shared/council_oneshot_index.txt \
  /shared/techauth.oneshot.utxo \
  /shared/techauth_oneshot_hash.txt \
  /shared/techauth_oneshot_index.txt \
  /runtime-values/council.oneshot.utxo \
  /runtime-values/council_oneshot_hash.txt \
  /runtime-values/council_oneshot_index.txt \
  /runtime-values/techauth.oneshot.utxo \
  /runtime-values/techauth_oneshot_hash.txt \
  /runtime-values/techauth_oneshot_index.txt

chmod u+rw \
  /runtime-values/genesis.utxo \
  /runtime-values/mc.env \
  /shared/genesis.utxo \
  /shared/council.oneshot.utxo \
  /shared/council_oneshot_hash.txt \
  /shared/council_oneshot_index.txt \
  /shared/techauth.oneshot.utxo \
  /shared/techauth_oneshot_hash.txt \
  /shared/techauth_oneshot_index.txt \
  /runtime-values/council.oneshot.utxo \
  /runtime-values/council_oneshot_hash.txt \
  /runtime-values/council_oneshot_index.txt \
  /runtime-values/techauth.oneshot.utxo \
  /runtime-values/techauth_oneshot_hash.txt \
  /runtime-values/techauth_oneshot_index.txt

if [ -f "/shared/genesis.utxo" ]; then
touch /shared/cardano.ready
else
echo "Genesis UTXO file not found. Exiting..."
exit 1
fi
echo "Cardano chain is ready. Starting DB-Sync..."

wait
