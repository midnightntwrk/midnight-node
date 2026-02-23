#!/usr/bin/env bash

# This file is part of midnight-node.
# Copyright (C) 2026 Midnight Foundation
# SPDX-License-Identifier: Apache-2.0
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.

set -euo pipefail

# Network = testnet
export CARDANO_NODE_NETWORK_ID=42

# pass "alice" or "bob" as parameter to this script
NAME="${1:-alice}"

# Pick the UTxO that has cNight tokens (and ideally enough ADA for fees)
TXIN_TOKEN="cf65a7211f592e167e91c09bdbf9e8c679b396e9959b225b034f2f7e84a59268#1"

# Pick a UTxO that has enough ADA (can be the same as TXIN_TOKEN)
TXIN_ADA="cf65a7211f592e167e91c09bdbf9e8c679b396e9959b225b034f2f7e84a59268#1"

# Amount of ADA to keep in the output with tokens (min-ADA)
LOVELACE_OUT="2000000"

# Amount of cNight tokens to self-send
ASSET_AMOUNT_SEND="10"

# Asset id: policyId + "." (empty asset name), read from file
ASSET_ID="$(tr -d '[:space:]' < cnight_policy.hash)."

rm send-cnight-"$NAME".tx 2>/dev/null || true
rm send-cnight-"$NAME"-signed.tx 2>/dev/null || true

# Build transaction body, fees included (self-send to payment-$NAME.addr)
cardano-cli conway transaction build \
  --tx-in "$TXIN_TOKEN" \
  --tx-in "$TXIN_ADA" \
  --tx-out "$(< payment-$NAME.addr)+${LOVELACE_OUT} lovelace + ${ASSET_AMOUNT_SEND} ${ASSET_ID}" \
  --change-address "$(< payment-$NAME.addr)" \
  --out-file send-cnight-"$NAME".tx || exit

# Sign and submit
cardano-cli conway transaction sign \
  --tx-file send-cnight-"$NAME".tx \
  --signing-key-file payment-"$NAME".skey \
  --out-file send-cnight-"$NAME"-signed.tx || exit

cardano-cli conway transaction submit \
  --tx-file send-cnight-"$NAME"-signed.tx || exit

# Print hash of submitted transaction
cardano-cli conway transaction txid \
  --tx-file send-cnight-"$NAME"-signed.tx
