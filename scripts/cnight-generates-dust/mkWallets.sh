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

# Make sure not to overwrite Alice's wallet
if [[ !(-f payment-alice.vkey || -f payment-alice.skey) ]]; then
  # Generate a new pair of keys
  cardano-cli conway address key-gen \
    --verification-key-file payment-alice.vkey \
    --signing-key-file payment-alice.skey

  cardano-cli conway address key-gen \
    --verification-key-file payment-alice.vkey \
    --signing-key-file payment-alice.skey
  echo "Created wallet for Alice"
fi

# Make sure not to overwrite Bob's wallet
if [[ !(-f payment-bob.vkey || -f payment-bob.skey) ]]; then
  # Generate a new pair of keys
  cardano-cli conway address key-gen \
    --verification-key-file payment-bob.vkey \
    --signing-key-file payment-bob.skey

  cardano-cli conway address key-gen \
    --verification-key-file payment-bob.vkey \
    --signing-key-file payment-bob.skey
  echo "Created wallet for Bob"
fi

# Create address files
cardano-cli conway address build \
  --payment-verification-key-file payment-alice.vkey \
  --out-file payment-alice.addr \

cardano-cli conway address build \
  --payment-verification-key-file payment-bob.vkey \
  --out-file payment-bob.addr \

echo "Wallet address for Alice: `cat payment-alice.addr`"
echo "Wallet address for Bob  : `cat payment-bob.addr`"

# These values should go into datum-*.json files
echo "Base16 Alice: `cat payment-alice.addr | basenc --base16`"
echo "Base16 Bob  : `cat payment-bob.addr   | basenc --base16`"

# Save global variables, expecting this to be sourced
export ALICE_BASE16="$alice_base16"
export BOB_BASE16="$bob_base16"
