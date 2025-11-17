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
export CARDANO_NODE_NETWORK_ID=42

# pass "alice" or "bob" as parameter to this script

# Get the collateral UTxO
COLLATERAL=ea22bb7b5f8787dc31985fd86d3d209459018180486074165e29401705de8795#0

# Pick the first UTxO on the wallet that is not a collateral.
# THIS IS VERY ERROR PRONE AND I EXPECT IT TO BREAK EVENTUALLY
UTXO=52956f41579e93d8fb12847476915b2f33e4bb5e494d6e0843822b5d7544e93a#1

# This needs to be entered manually.  UTxO to spend can be obtained from
# cardanoscan.io or remembered from a registration transaction.  NOTE: at the
# moment smart contracts are just stubs, so it is possible for Alice to
# deregister Bob and vice versa.
REGISTRATION_UTXO=52956f41579e93d8fb12847476915b2f33e4bb5e494d6e0843822b5d7544e93a#0

USER_PKH=$(cardano-cli address key-hash --payment-verification-key-file payment-$1.vkey)

rm deregister-$1.tx 2>/dev/null
rm deregister-$1-signed.tx 2>/dev/null

# Build transaction body, fees included
cardano-cli conway transaction build \
  --tx-in $UTXO \
  --tx-in $REGISTRATION_UTXO \
  --tx-in-script-file auth_token_policy.plutus \
  --tx-in-redeemer-value "{}" \
  --tx-out $(< payment-$1.addr)+"2000000" \
  --tx-in-collateral $COLLATERAL \
  --mint="-1 $(< auth_token.hash)" \
  --mint-script-file auth_token_policy.plutus \
  --mint-redeemer-file deregister_red.json  \
  --change-address $(< payment-$1.addr) \
  --required-signer-hash $USER_PKH \
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
