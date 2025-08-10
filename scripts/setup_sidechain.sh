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

if [ -z "$GENESIS_UTXO" ]; then
    echo "Warning: GENESIS_UTXO must be set'"
    exit 1;
fi

# Absolute path of payment key file
if [ -z "$PAYMENT_SIGNING_KEY_FILE" ]; then
    echo "Warning: PAYMENT_SIGNING_KEY_FILE is not set. Set to absolute path of file"
    exit 1;
fi

if [ -z "$MOCK_FILE" ]; then
    echo "Warning: MOCK_FILE is not set"
    echo "Using env-vars instead..."

    if [ -z "$D_PARAMETER_PERMISSIONED_CANDIDATES_COUNT" ]; then
        echo "Warning: D_PARAMETER_PERMISSIONED_CANDIDATES_COUNT is not set"
        exit 1;
    fi

    if [ -z "$D_PARAMETER_REGISTERED_CANDIDATES_COUNT" ]; then
        echo "Warning: D_PARAMETER_REGISTERED_CANDIDATES_COUNT is not set"
        exit 1;
    fi

    # Add Permissioned candidates in here in the order sidechains_pub_key:aura_pub_key:grandpa_pub_key
    # PERMISSIONED_CANDIDATES=(
    #     "sidechains_pub_key:aura_pub_key:grandpa_pub_key" \
    #     "sidechains_pub_key:aura_pub_key:grandpa_pub_key"
    # )

else
    if [ ! -f $MOCK_FILE ]; then
        echo "Error: MOCK_FILE \"$MOCK_FILE\" does not exist"
        exit 1;
    fi

    echo "Reading D parameters and permissioned candidates from mock file: $MOCK_FILE"

    D_PARAMETER_PERMISSIONED_CANDIDATES_COUNT=$(jq '.[0].d_parameter.permissioned' $MOCK_FILE)
    D_PARAMETER_REGISTERED_CANDIDATES_COUNT=$(jq '.[0].d_parameter.registered' $MOCK_FILE)
    PERMISSIONED_CANDIDATES=($(jq -r '.[0].permissioned[] | .sidechain_pub_key + ":" + .aura_pub_key + ":" + .grandpa_pub_key' $MOCK_FILE))
fi

# Run prerequisite services
echo "Running prerequisite services..."
# ../partnerchains-dev.sh

# Generate addresses with Trustless CLI
echo "Generating addresses with Trustless CLI..."
docker exec -it partnerchains-dev ./pc-contracts-cli addresses \
  --genesis-utxo $GENESIS_UTXO \
  --payment-signing-key-file $PAYMENT_SIGNING_KEY_FILE \
  --network testnet \
  --ogmios-host ogmios.preview.midnight.network \
  --ogmios-port 443 \
  --ogmios-secure \
  --kupo-host kupo.preview.midnight.network \
  --kupo-port 443 \
  --kupo-secure \
  > addresses.json

# Init governance (creates governance-related NFTs, needed only once)
docker exec -it partnerchains-dev ./pc-contracts-cli init-governance \
  --genesis-utxo $GENESIS_UTXO \
  --governance-authority $GOVERNACE_AUTHORITY \
  --payment-signing-key-file $PAYMENT_SIGNING_KEY_FILE \
  --network testnet \
  --ogmios-host ogmios.preview.midnight.network \
  --ogmios-port 443 \
  --ogmios-secure \
  --kupo-host kupo.preview.midnight.network \
  --kupo-port 443 \
  --kupo-secure

# Set Ariadne parameters
echo "Setting Ariadne parameters..."
docker exec -it partnerchains-dev ./pc-contracts-cli insert-d-parameter \
  --genesis-utxo $GENESIS_UTXO \
  --d-parameter-permissioned-candidates-count $D_PARAMETER_PERMISSIONED_CANDIDATES_COUNT \
  --d-parameter-registered-candidates-count $D_PARAMETER_REGISTERED_CANDIDATES_COUNT \
  --payment-signing-key-file $PAYMENT_SIGNING_KEY_FILE \
  --network testnet \
  --ogmios-host ogmios.preview.midnight.network \
  --ogmios-port 443 \
  --ogmios-secure \
  --kupo-host kupo.preview.midnight.network \
  --kupo-port 443 \
  --kupo-secure

# Loop through each candidate in the array first
add_candidates_params=""

for CANDIDATE in "${PERMISSIONED_CANDIDATES[@]}"; do
  add_candidates_params+=" --add-candidate $CANDIDATE"
done

# Insert the list of all permissioned candidates
echo "Inserting the list of all permissioned candidates..."
  docker exec -it partnerchains-dev ./pc-contracts-cli update-permissioned-candidates \
    --genesis-utxo $GENESIS_UTXO \
    --payment-signing-key-file $PAYMENT_SIGNING_KEY_FILE \
    $add_candidates_params \
    --network testnet \
    --ogmios-host ogmios.preview.midnight.network \
    --ogmios-port 443 \
    --ogmios-secure \
    --kupo-host kupo.preview.midnight.network \
    --kupo-port 443 \
    --kupo-secure \

echo "All steps completed successfully!"
