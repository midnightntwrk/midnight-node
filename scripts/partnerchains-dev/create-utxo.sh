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

# This script splits an existing UTXO and creates two new UTXOs: one with 5 ADA and one with the remaining amount.
# It takes 4 arguments:
# 1. Payment signing key
# 2. Payment address
# 3. Input transaction hash and index
# 4. Input amount
# Example:
# ./create-utxo.sh payment.skey addr_test1vz7v2w3 01b618fce98303d9cfab117342b52abcf5387d2c62bd2c529da5b72eecba97a9#1 10000000

# First argument is transaction in, second argument is amount - if it doesn't exist, print usage
if [ -z "$4" ]; then
  echo "This script splits an existing UTXO and creates two new UTXOs: one with 5 ADA and one with the remaining amount."
  echo "Usage: $0 <payment_skey> <payment_addr> <input_tx_hash>#<input_tx_index> <current_amount> <desired_amount>"
  echo "Example: $0 res/testnet-02/governance.skey \$(< res/testnet-02/governance.addr) 01b618fce98303d9cfab117342b52abcf5387d2c62bd2c529da5b72eecba97a9#1 9975470361 20000000"
  exit 1
fi

PAYMENT_SKEY=$1
PAYMENT_ADDR=$2
TX_IN=$3
AMOUNT=$4

SMALLEST_UTXO=$5

set -x

./cardano-cli conway query protocol-parameters --out-file pparams.json

# Create a UTXO with 5 ADA
OTHER_AMOUNT=$(($AMOUNT - $SMALLEST_UTXO))
./cardano-cli conway transaction build-raw \
  --tx-in $TX_IN \
  --tx-out $PAYMENT_ADDR+$SMALLEST_UTXO \
  --tx-out $PAYMENT_ADDR+$OTHER_AMOUNT \
  --fee 0 \
  --out-file tx.raw

FEE=$(
    ./cardano-cli conway transaction calculate-min-fee \
        --tx-body-file tx.raw \
        --witness-count 1 \
        --protocol-params-file pparams.json | awk '{print $1}'
)


# Rebuild tx with fee
OTHER_AMOUNT=$(($AMOUNT - $SMALLEST_UTXO - $FEE))
./cardano-cli conway transaction build-raw \
  --tx-in $TX_IN \
  --tx-out $PAYMENT_ADDR+$SMALLEST_UTXO \
  --tx-out $PAYMENT_ADDR+$OTHER_AMOUNT \
  --fee $FEE \
  --out-file tx.raw

# Sign the transaction
./cardano-cli conway transaction sign \
  --tx-body-file tx.raw \
  --signing-key-file $PAYMENT_SKEY \
  --out-file tx.signed

echo "Output transaction: tx.signed"
echo "Use the following command to submit the transaction:"
echo "./cardano-cli conway transaction submit --tx-file tx.signed"
