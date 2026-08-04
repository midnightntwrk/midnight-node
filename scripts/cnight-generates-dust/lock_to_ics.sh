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

# Mirrors tests/e2e/src/api/cardano.rs::make_bridge_transfer:
# locks cNIGHT at the ICS validator with an inline unit datum and tx metadata
# (label 6500973) carrying the 32-byte Midnight recipient address as bytes.

set -euo pipefail

# Network = testnet
export CARDANO_NODE_NETWORK_ID=42

# pass "alice" or "bob" as parameter to this script
NAME="${1:-alice}"

# Input UTxO holding cNIGHT tokens to lock
CNIGHT_UTXO=eb0c7e90d54c87cf19965225b71a27cf8a4fc77b88e858ed146f78feb3294037#0

# Input UTxO covering fees (also used as the collateral input, matching the
# rust test which reuses the payment UTxO for both roles)
PAYMENT_UTXO=84b251f90c63e43fa38f4b6703f826ce5a11fc73003d0ac5e9fd324e1c71e4c7#1

# ICS validator bech32 address (bridge target). Populate ics_validator.addr
# from the running node's bridge pallet storage, or override inline below.
ICS_ADDRESS=addr_test1wp9a24gezjgwhnt6a7tdef24xnqqcdzjzyf5u3q4urs2m7qeuln0n

# 32-byte (64 hex chars) Midnight recipient address — gets wrapped into a
# bytes metadatum inside a single-element list at label 6500973.
RECIPIENT_HEX=60241462a2a18b13aeb6451ff3c416c3942be95010faa5dc72c55be6b258fb03

# Amount of cNIGHT tokens (STARS) to lock to the ICS validator
AMOUNT=50000000

# Lovelace bundled with the cNIGHT output at the ICS address (min-UTxO)
LOVELACE_OUT=1500000

# Metadata label used by the bridge — matches BRIDGE_METADATUM_LABEL in
# tests/e2e/src/api/cardano.rs and TOKEN_TRANSFER_METADATUM_KEY in
# partner-chains/toolkit/smart-contracts/plutus-data/src/bridge.rs.
METADATUM_LABEL=6500973

TX_BODY=lock-to-ics-$NAME.tx
TX_SIGNED=lock-to-ics-$NAME-signed.tx
rm -f "$TX_BODY" "$TX_SIGNED"

# Bridge metadatum: `list [ bytes(recipient) ]` at the bridge label.
METADATA_FILE=$(mktemp -t lock-to-ics-metadata.XXXXXX)
trap 'rm -f "$METADATA_FILE"' EXIT
cat > "$METADATA_FILE" <<EOF
{
  "$METADATUM_LABEL": {
    "list": [
      { "bytes": "$RECIPIENT_HEX" }
    ]
  }
}
EOF

# Build transaction body, fees included
cardano-cli conway transaction build \
  --tx-in "$CNIGHT_UTXO" \
  --tx-in "$PAYMENT_UTXO" \
  --tx-in-collateral "$PAYMENT_UTXO" \
  --tx-out "$ICS_ADDRESS+$LOVELACE_OUT lovelace + $AMOUNT $(< cnight_policy.hash)" \
  --tx-out-inline-datum-value '{"constructor": 0, "fields": []}' \
  --metadata-json-file "$METADATA_FILE" \
  --json-metadata-detailed-schema \
  --change-address "$(< payment-$NAME.addr)" \
  --out-file "$TX_BODY" || exit

# Sign and submit
cardano-cli conway transaction sign \
  --tx-file "$TX_BODY" \
  --signing-key-file "payment-$NAME.skey" \
  --out-file "$TX_SIGNED" || exit

# cardano-cli conway transaction submit \
#   --tx-file "$TX_SIGNED" || exit

# Print hash of submitted transaction
cardano-cli conway transaction txid \
  --tx-file "$TX_SIGNED"

echo "transaction signed but not yet submitted - add to allowlist first."
