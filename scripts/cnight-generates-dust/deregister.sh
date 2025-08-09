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

# Network = testnet
export CARDANO_NODE_NETWORK_ID=2

# pass "alice" or "bob" as parameter to this script

# Get the collateral UTxO
COLLATERAL=$(< collateral-$1.utxo)

# Pick the first UTxO on the wallet that is not a collateral.
# THIS IS VERY ERROR PRONE AND I EXPECT IT TO BREAK EVENTUALLY
UTXO=$(cardano-cli conway query utxo --address $(< payment-$1.addr) --output-json | jq -r 'keys' | jq '.[]' | grep -v $COLLATERAL | head -n1 | tr -d '"')

# This needs to be entered manually.  UTxO to spend can be obtained from
# cardanoscan.io or remembered from a registration transaction.  NOTE: at the
# moment smart contracts are just stubs, so it is possible for Alice to
# deregister Bob and vice versa.
REGISTRATION_UTXO=2f20ab66106104478df2c57e5f86c295840df75ff136ed1d9af6d2da0c52b97b#0

rm deregister-$1.tx 2>/dev/null
rm deregister-$1-signed.tx 2>/dev/null

# Build transaction body, fees included
cardano-cli conway transaction build \
  --tx-in $UTXO \
  --tx-in $REGISTRATION_UTXO \
  --tx-in-script-file mapping_validator.plutus \
  --tx-in-redeemer-value "{}" \
  --tx-out $(< payment-$1.addr)+"20000000" \
  --tx-in-collateral $COLLATERAL \
  --mint="-1 $(< auth_token.hash)" \
  --mint-script-file auth_token_policy.plutus \
  --mint-redeemer-file deregister_red.json  \
  --change-address $(< payment-$1.addr) \
  --out-file deregister-$1.tx || exit

# Sign and submit
cardano-cli conway transaction sign \
  --tx-file deregister-$1.tx \
  --signing-key-file payment-$1.skey \
  --out-file deregister-$1-signed.tx || exit

cardano-cli conway transaction submit \
  --tx-file deregister-$1-signed.tx || exit

# Print hash of submitted transaction
cardano-cli conway transaction txid \
  --tx-file deregister-$1-signed.tx
