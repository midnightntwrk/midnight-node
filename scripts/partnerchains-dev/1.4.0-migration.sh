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

# First argument is network name - if it doesn't exist, print usage
if [ -z "$1" ]; then
    echo "Usage: $0 <network-name>"
    exit 1
fi

NETWORK=$1

# Regenerate PC config files based on current ones
./update-network.sh $NETWORK

# If everything went fine, the new `partner-chains-cli-chain-config.json` and
# chain spec files should be in the root
CONFIG_FILE=partner-chains-cli-chain-config.json
CHAIN_SPEC=chain-spec.json

# Check PC config file exists
if [ ! -f "$CONFIG_FILE" ]; then
    echo "Chain config file $CONFIG_FILE does not exist."
    exit 1
fi

# Check the chain spec file exists
if [ ! -f "$CHAIN_SPEC" ]; then
    echo "Chain spec $CHAIN_SPEC does not exist."
    exit 1
fi

# Remove the unused `chain_parameters` after migration from the config file 
jq 'del(
  .chain_parameters.chain_id, 
  .chain_parameters.genesis_committee_utxo, 
  .chain_parameters.threshold_numerator, 
  .chain_parameters.threshold_denominator, 
  .chain_parameters.governance_authority
)' "$CONFIG_FILE" > tmp-cfg.json && mv tmp-cfg.json "$CONFIG_FILE"

# Copy PC configutation and chain specs files to local host
MIGRATION_DIR=/res/$NETWORK/1.4.0-migration
mkdir -p $MIGRATION_DIR
cp $CONFIG_FILE $MIGRATION_DIR
cp $CHAIN_SPEC $MIGRATION_DIR
