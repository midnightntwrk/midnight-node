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
COLLATERAL=d1e3850679fafcea533f97eed4cdd58d563719f319d1e3c21f2225f8ef4f65a9#0

# Pick the first UTxO on the wallet that is not a collateral.
# THIS IS VERY ERROR PRONE AND I EXPECT IT TO BREAK EVENTUALLY
UTXO=d1e3850679fafcea533f97eed4cdd58d563719f319d1e3c21f2225f8ef4f65a9#1

rm receive-cnight-$1.tx 2>/dev/null
rm receive-cnight-$1-signed.tx 2>/dev/null

# Build transaction body, fees included
cardano-cli conway transaction build \
  --tx-in $UTXO \
  --tx-out $(< payment-$1.addr)+"1500000 lovelace + 10 $(< cnight_policy.hash)" \
  --tx-in-collateral $COLLATERAL \
  --mint="10 $(< cnight_policy.hash)" \
  --mint-script-file cnight_policy.plutus \
  --mint-redeemer-value "{}" \
  --change-address $(< payment-$1.addr) \
  --out-file receive-cnight-$1.tx || exit

# Sign and submit
cardano-cli conway transaction sign \
  --tx-file receive-cnight-$1.tx \
  --signing-key-file payment-$1.skey \
  --out-file receive-cnight-$1-signed.tx || exit

cardano-cli conway transaction submit \
  --tx-file receive-cnight-$1-signed.tx || exit

# Print hash of submitted transaction
cardano-cli conway transaction txid \
  --tx-file receive-cnight-$1-signed.tx
