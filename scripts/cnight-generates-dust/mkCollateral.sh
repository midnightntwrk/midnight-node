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

UTXO=21b584b9ccab27c27fa0f0f891d9fbdcb18e4ad28665d4eb17b26e41ff190bc4#1

rm collateral-$1.tx 2>/dev/null
rm collateral-$1-signed.tx 2>/dev/null

# Build transaction body, fees included
cardano-cli conway transaction build \
  --tx-in $UTXO \
  --tx-out $(< payment-$1.addr)+"5000000 lovelace" \
  --change-address $(< payment-$1.addr) \
  --out-file collateral-$1.tx || exit

cardano-cli conway transaction sign \
  --tx-file collateral-$1.tx \
  --signing-key-file payment-$1.skey \
  --out-file collateral-$1-signed.tx || exit

cardano-cli conway transaction submit \
  --tx-file collateral-$1-signed.tx || exit

# Save collateral to a file
COLLATERAL=$(cardano-cli conway transaction txid \
  --tx-file collateral-$1-signed.tx)"#0"
echo $COLLATERAL > collateral-$1.utxo
