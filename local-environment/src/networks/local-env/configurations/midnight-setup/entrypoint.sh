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

# Step 3 of chain preparation: build the chain spec the nodes boot from, out of the
# configs patched by patch-configs.sh and the genesis built by generate-genesis.sh,
# then hold the nodes back until the Cardano D-parameter is active.

# Fail if a command fails. (No -x: the script narrates every step itself, so xtrace
# would just double every line with '+' noise.)
set -euo pipefail

source /midnight-setup/common.sh

# Read contracts-active-epoch saved by contract-compiler
contracts_active_epoch=$(cat /runtime-values/contracts-active-epoch)
echo "Contracts will be active at epoch: $contracts_active_epoch"

echo "Using Partner Chains node version:"
./midnight-node --version

# Genesis blobs produced by generate-genesis.sh with the toolkit image that matches
# this node image. Overridable, but the defaults are what midnight-setup-genesis writes.
GENESIS_STATE="${CHAINSPEC_GENESIS_STATE:-/shared/genesis/genesis_state_local.mn}"
GENESIS_BLOCK="${CHAINSPEC_GENESIS_BLOCK:-/shared/genesis/genesis_block_local.mn}"

phase "Building chain-spec"

# Reuse the spec in the shared volume when there is one. Regenerating it rebuilds genesis
# from the (possibly changed) configs and node image, which changes the genesis hash — the
# nodes would then reject the chain data already in the volume as belonging to a different
# chain. That makes in-place node/runtime upgrades on a running local-env impossible.
# To force a fresh chain, drop the volume: `docker compose down -v`.
if [ -f /shared/chain-spec.json ]; then
  echo "/shared/chain-spec.json already exists — reusing it and skipping generation."
  check_json_validity /shared/chain-spec.json
else
  # Most chainspec inputs come from the `local` cfg preset (res/cfg/local.toml): the
  # chainspec_* paths there are relative (res/local/...) and the image workdir is /, so
  # they resolve through the /res repo mount — build-spec reads the configs patched by
  # patch-configs.sh without a node-image rebuild.
  #
  # The genesis blobs are the exception: res/cfg/local.toml points at the committed
  # res/genesis/genesis_*_local.mn, which are serialized for the ledger version of the
  # *checkout* and so are unreadable by an older ${MIDNIGHT_NODE_IMAGE}. Point them at
  # the blobs generate-genesis.sh built with the matching toolkit instead. The node
  # config layer reads unprefixed env vars as cfg keys (config::Environment::default,
  # see node/src/cfg/mod.rs), so these two exports override the preset's paths.
  for f in "$GENESIS_STATE" "$GENESIS_BLOCK"; do
    if [ ! -f "$f" ]; then
      echo "Error: genesis file $f is missing — did midnight-setup-genesis run?"
      exit 1
    fi
  done
  echo "Using genesis from the shared volume:"
  echo "  $GENESIS_STATE ($(genesis_tag "$GENESIS_STATE"))"
  echo "  $GENESIS_BLOCK"

  export CFG_PRESET=local
  export CHAINSPEC_GENESIS_STATE="$GENESIS_STATE"
  export CHAINSPEC_GENESIS_BLOCK="$GENESIS_BLOCK"

  ./midnight-node build-spec --disable-default-bootnode > chain-spec.json
  echo "chain-spec.json file generated."

  echo "Amending the chain spec..."
  echo "Configuring Epoch Length..."
  jq '.genesis.runtimeGenesis.config.sidechain.slotsPerEpoch = 5' chain-spec.json > tmp.json && mv tmp.json chain-spec.json

  check_json_validity chain-spec.json

  echo "Final chain spec"

  echo "Copying chain-spec.json file to /shared/chain-spec.json..."
  cp chain-spec.json /shared/chain-spec.json
  echo "chain-spec.json generation complete."
fi

echo "Partnerchain configuration is complete, and will be able to start after two mainchain epochs."

phase "Awaiting activation"

echo "Waiting for contracts to become active at epoch $contracts_active_epoch..."
epoch=$(curl -s --request POST \
    --url "http://ogmios:1337" \
    --header 'Content-Type: application/json' \
    --data '{"jsonrpc": "2.0", "method": "queryLedgerState/epoch"}' | jq .result)
echo "Current epoch: $epoch"
while [ "$epoch" -lt "$contracts_active_epoch" ]; do
  sleep 10
  epoch=$(curl -s --request POST \
    --url "http://ogmios:1337" \
    --header 'Content-Type: application/json' \
    --data '{"jsonrpc": "2.0", "method": "queryLedgerState/epoch"}' | jq .result)
  echo "Current epoch: $epoch"
done
echo "DParam is now active!"
