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
funded_address=$(cat /shared/FUNDED_ADDRESS)
registration_utxo=$(curl -fsS -H 'Content-Type: application/json' \
  -d "$(jq -nc --arg address "$funded_address" \
    '{jsonrpc:"2.0",method:"queryLedgerState/utxo",params:{addresses:[$address]},id:1}')" \
  "http://ogmios:$OGMIOS_PORT" \
  | jq -er '.result | map(select((.value | keys) == ["ada"])) | max_by(.value.ada.lovelace)
      | .transaction.id + "#" + (.index | tostring)')
printf '%s\n' "$registration_utxo" > /shared/pool1-registration-utxo

./midnight-node registration-signatures \
  --genesis-utxo "$(jq -r '.chain_parameters.genesis_utxo' /res/local/pc-chain-config.json)" \
  --mainchain-signing-key "$(jq -r '.cborHex[4:]' /keys/cold.skey)" \
  --sidechain-signing-key "$(cat /seeds/cross_chain.seed)" \
  --registration-utxo "$registration_utxo" > /shared/pool1-signatures.json
