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

set -euo pipefail

[ ! -s /runtime-values/pool1-registered ] || exit 0
candidate_address=$(jq -er '.[] | select(.name == "Registered Candidate") | .address' /runtime-values/contracts-info.json)
funded_address=$(cat /shared/FUNDED_ADDRESS)
registration_utxo=$(cat /shared/pool1-registration-utxo)
owner=$(cardano-cli latest address key-hash --payment-verification-key-file /keys/funded_address.vkey)

jq -n --arg owner "$owner" --arg utxo "$registration_utxo" \
  --slurpfile signatures /shared/pool1-signatures.json --slurpfile keys /public-keys.json \
  -f /pool1-registration/datum.jq > /shared/pool1-registration-datum.json
cardano-cli latest transaction build --testnet-magic 42 \
  --tx-in "$registration_utxo" \
  --tx-out "$candidate_address+3000000" \
  --tx-out-inline-datum-file /shared/pool1-registration-datum.json \
  --change-address "$funded_address" --out-file /tmp/pool1-registration.raw
cardano-cli latest transaction sign --testnet-magic 42 \
  --tx-body-file /tmp/pool1-registration.raw \
  --signing-key-file /keys/funded_address.skey --out-file /tmp/pool1-registration.signed
cardano-cli latest transaction submit --testnet-magic 42 --tx-file /tmp/pool1-registration.signed
registration_tx=$(cardano-cli latest transaction txid --tx-file /tmp/pool1-registration.signed | jq -r .txhash)

until cardano-cli latest query utxo --testnet-magic 42 --address "$candidate_address" --out-file /tmp/pool1-utxos.json \
  && jq -e --arg tx "$registration_tx#0" 'has($tx)' /tmp/pool1-utxos.json > /dev/null; do
  sleep 2
done
epoch=$(cardano-cli latest query tip --testnet-magic 42 | jq -r '.epoch')
active_epoch=$((epoch + 2))
contracts_active_epoch=$(cat /runtime-values/contracts-active-epoch)
if [ "$active_epoch" -gt "$contracts_active_epoch" ]; then
  printf '%s\n' "$active_epoch" > /runtime-values/contracts-active-epoch
fi
printf '%s\n' "$registration_tx" > /runtime-values/pool1-registered
