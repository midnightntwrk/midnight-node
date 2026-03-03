#!/usr/bin/env bash

# This file is part of midnight-node.
# Copyright (C) Midnight Foundation
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

# Make sure not to overwrite Alice's wallet
if [[ !(-f payment-alice.vkey || -f payment-alice.skey) ]]; then
  # Generate a new pair of keys
  cardano-cli conway address key-gen \
    --verification-key-file payment-alice.vkey \
    --signing-key-file payment-alice.skey

  cardano-cli conway stake-address key-gen \
    --verification-key-file stake-alice.vkey \
    --signing-key-file stake-alice.skey
  echo "Created wallet for Alice"
fi

# Make sure not to overwrite Bob's wallet
if [[ !(-f payment-bob.vkey || -f payment-bob.skey) ]]; then
  # Generate a new pair of keys
  cardano-cli conway address key-gen \
    --verification-key-file payment-bob.vkey \
    --signing-key-file payment-bob.skey

  cardano-cli conway stake-address key-gen \
    --verification-key-file stake-bob.vkey \
    --signing-key-file stake-bob.skey
  echo "Created wallet for Bob"
fi

# Create address files
cardano-cli conway address build \
  --payment-verification-key-file payment-alice.vkey \
  --stake-verification-key-file stake-alice.vkey \
  --out-file payment-alice.addr

cardano-cli conway stake-address build \
    --stake-verification-key-file stake-alice.vkey \
    --out-file stake-alice.addr

cardano-cli conway address build \
  --payment-verification-key-file payment-bob.vkey \
  --stake-verification-key-file stake-bob.vkey \
  --out-file payment-bob.addr

cardano-cli conway stake-address build \
    --stake-verification-key-file stake-bob.vkey \
    --out-file stake-bob.addr

echo "Wallet address for Alice: `cat payment-alice.addr`"
echo "Wallet address for Bob  : `cat payment-bob.addr`"

echo "Stake address for Alice  : `cat stake-alice.addr`"
echo "Stake address for Bob    : `cat stake-bob.addr`"

echo "Stake PKH for Alice   : `cardano-cli address key-hash --payment-verification-key-file stake-alice.vkey`"
echo "Stake PKH for Bob     : `cardano-cli address key-hash --payment-verification-key-file stake-bob.vkey`"

echo "Update datum-*.json files with the above PKHs before proceeding."
echo "Done."
