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

WALLET_DIR="$PWD/governance-wallet"

mkdir -p $WALLET_DIR

./cardano-cli address key-gen \
--verification-key-file $WALLET_DIR/payment.vkey \
--signing-key-file $WALLET_DIR/payment.skey

./cardano-cli address build \
--payment-verification-key-file $WALLET_DIR/payment.vkey \
--out-file $WALLET_DIR/payment.addr \

echo "Wallet created. Files saved to: $WALLET_DIR"
echo "Address: $(cat $WALLET_DIR/payment.addr)"
