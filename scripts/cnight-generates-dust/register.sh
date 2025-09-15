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

# Get the collateral UTxO. This should be entered manually
COLLATERAL=660f50990052202d4f07ed20d2bf945fae53fb9a759e05c879f10ffd1132b229#0

# Pick the first UTxO on the wallet that is not a collateral. This should be entered manually
UTXO=660f50990052202d4f07ed20d2bf945fae53fb9a759e05c879f10ffd1132b229#1

rm register-$1.tx 2>/dev/null
rm register-$1-signed.tx 2>/dev/null

# Build transaction body, fees included
cardano-cli conway transaction build \
  --tx-in $UTXO \
  --tx-out $(< mapping_validator.addr)+"150000000 lovelace + 1 $(< auth_token.hash)" \
  --tx-out-inline-datum-file datum-$1.json \
  --tx-in-collateral $COLLATERAL \
  --mint="1 $(< auth_token.hash)" \
  --mint-script-file auth_token_policy.plutus \
  --mint-redeemer-file register_red.json  \
  --change-address $(< payment-$1.addr) \
  --out-file register-$1.tx || exit

# Sign and submit
cardano-cli conway transaction sign \
  --tx-file register-$1.tx \
  --signing-key-file payment-$1.skey \
  --out-file register-$1-signed.tx || exit

cardano-cli conway transaction submit \
  --tx-file register-$1-signed.tx || exit

# Print hash of submitted transaction
cardano-cli conway transaction txid \
  --tx-file register-$1-signed.tx
