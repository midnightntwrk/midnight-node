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

if [ -z "$1" ]; then
  echo "Usage: $0 <network-name>"
  exit 1
fi
NETWORK=$1
CHAIN_CONFIG="res/$NETWORK/partner-chains-cli-chain-config.json"
GOVERNANCE_SECRET="res/$NETWORK/governance.skey"
GENESIS_UTXO=$(jq -r '.chain_parameters.genesis_utxo' < "$GENESIS_UTXO")

set +x

(
./pc-contracts-cli addresses \
    --genesis-utxo $GENESIS_UTXO \
	--payment-signing-key-file "$GOVERNANCE_SECRET" \
	--atms-kind plain-ecdsa-secp256k1 \
	--ogmios-host ogmios.preview.midnight.network \
	--ogmios-port 443 \
	--ogmios-secure \
	--kupo-host kupo.preview.midnight.network \
	--kupo-port 443 \
	--kupo-secure \
	--network testnet | jq > "res/$NETWORK/addresses.json"
)

echo "Wrote addresses to res/$NETWORK/addresses.json"
