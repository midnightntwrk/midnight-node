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

# Step 2 of chain preparation: build the genesis ledger state for this bring-up with
# the toolkit image, and hand it to build-spec through the shared volume.
#
# Why not just use the committed res/genesis/genesis_*_local.mn? Those blobs carry a
# version-bound serialization tag (`midnight:ledger-state[v18]:...`) and are rebuilt
# from the checkout, while ${MIDNIGHT_NODE_IMAGE} may be any released image: a v18 blob
# is undeserializable by a node that speaks v13 (node-0.22.x, node-1.0.x), so an older
# image could not boot local-env at all. Generating here with ${TOOLKIT_IMAGE} — which
# is pinned alongside the node image — makes the genesis match whatever image is
# running, old or new.
#
# This mirrors `earthly +rebuild-genesis-state-local` (Earthfile `rebuild-genesis-state`,
# FUND_FAUCET_WALLETS=false): the local network funds no wallets at genesis, all NIGHT
# arrives later over the cNIGHT->mNIGHT bridge. Only the ICS/reserve *totals* reach the
# ledger state, so running after patch-configs.sh (totals taken from the cNIGHT actually
# seeded on Cardano) reproduces the committed blob byte for byte, while keeping the
# `C.* == M.*` pool invariant true by construction rather than by hand-synced constants.

set -euo pipefail

source /midnight-setup/common.sh

NETWORK="${GENESIS_NETWORK:-local}"
GENESIS_DIR="${GENESIS_DIR:-/shared/genesis}"
TOOLKIT_BIN="${TOOLKIT_BIN:-/midnight-node-toolkit}"
LEGACY_FIELDS=/midnight-setup/genesis-compat/ledger-parameters-legacy-fields.json
WORK_DIR=/tmp/genesis-config
GENESIS_STATE="${GENESIS_DIR}/genesis_state_${NETWORK}.mn"
GENESIS_BLOCK="${GENESIS_DIR}/genesis_block_${NETWORK}.mn"

phase "Generating genesis"

# The chain spec pins the genesis it was built with, so once one exists the chain's
# genesis is already decided; regenerating would only risk a different genesis hash
# for the node data already in the volume. Same reasoning as the build-spec reuse in
# entrypoint.sh. To force a fresh chain, drop the volume: `docker compose down -v`.
if [ -f /shared/chain-spec.json ]; then
  echo "/shared/chain-spec.json already exists — genesis is already fixed; skipping generation."
  exit 0
fi

# The toolkit CLI has no --version; the image records the release it was built from.
if [ -f /node_version ]; then
  echo "Toolkit image version: $(cat /node_version)"
fi

mkdir -p "$GENESIS_DIR"

# res/local configs track the checkout, so a toolkit older than the checkout can reject
# them over fields it still requires (node-1.0.x: `cost_model.parallelism_factor`). Fill
# those gaps from the legacy-fields overlay; res/local always wins on conflicts, and a
# toolkit that no longer knows a field ignores it — verified byte-identical genesis on
# the current toolkit with and without the overlay. Add a field here when an older
# toolkit rejects the config; the other three configs have not drifted.
LEDGER_PARAMETERS=/res/local/ledger-parameters-config.json
if [ -f "$LEGACY_FIELDS" ]; then
  mkdir -p "$WORK_DIR"
  jq -s '(.[1] | del(._comment)) * .[0]' "$LEDGER_PARAMETERS" "$LEGACY_FIELDS" \
    > "$WORK_DIR/ledger-parameters-config.json"
  LEDGER_PARAMETERS="$WORK_DIR/ledger-parameters-config.json"
  echo "Ledger parameters: res/local + $(basename "$LEGACY_FIELDS")"
fi

# --allow-empty-pools is deliberately not passed: empty ICS/reserve pools mean the
# cNIGHT seeding on Cardano did not land, and that should fail the bring-up loudly
# here rather than produce a chain with no NIGHT in it. (The flag also does not exist
# on older toolkits, so not passing it keeps this call portable across versions.)
"$TOOLKIT_BIN" generate-genesis \
  --network "$NETWORK" \
  --ledger-parameters-config "$LEDGER_PARAMETERS" \
  --cnight-generates-dust-config /res/local/cnight-config.json \
  --ics-config /res/local/ics-config.json \
  --reserve-config /res/local/reserve-config.json \
  -o "$GENESIS_DIR"

[ -f "$GENESIS_STATE" ] || { echo "Error: toolkit did not write $GENESIS_STATE"; exit 1; }
[ -f "$GENESIS_BLOCK" ] || { echo "Error: toolkit did not write $GENESIS_BLOCK"; exit 1; }

echo "Genesis written to $GENESIS_DIR:"
ls -l "$GENESIS_DIR"
echo "genesis state tag: $(genesis_tag "$GENESIS_STATE")"
