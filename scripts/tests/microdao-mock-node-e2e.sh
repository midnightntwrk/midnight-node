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

# Micro-dao e2e on the mock-proof fast lane: a native single-authority node built with
# `--features mock-proofs` (no Docker, no Cardano) and the toolkit emitting mock contract and
# DUST proofs. Runs the flow of microdao-local-env-e2e.sh without contract proving or the
# local-env bring-up.
#
#   OUTDIR       artifacts (default artifacts/microdao-mock-e2e)
#   FAST_BLOCKS  1 (default) builds the node with `fast-blocks` (2s blocks); 0 keeps 6s
#   RPC_PORT     node RPC port (default 9977; p2p and prometheus use RPC_PORT+1/+2)

set -euo pipefail

# shellcheck source=scripts/tests/microdao/lib.sh
source "$(dirname "${BASH_SOURCE[0]}")/microdao/lib.sh"

OUTDIR=${OUTDIR:-$REPO_ROOT/artifacts/microdao-mock-e2e}
RPC_PORT=${RPC_PORT:-9977}
NODE_FEATURES=mock-proofs
[[ ${FAST_BLOCKS:-1} == 1 ]] && NODE_FEATURES+=,fast-blocks
NODE_BIN=$REPO_ROOT/target/release/midnight-node
NETWORK=undeployed

build() {
    cd "$REPO_ROOT"
    eval "$(direnv export bash)"
    cargo build --release -p midnight-node --features "$NODE_FEATURES"
    build_toolkit
}

start_node() {
    cd "$REPO_ROOT"   # res/cfg paths are relative to the working directory
    # An empty `--chain=` builds the chain spec from CHAINSPEC_GENESIS_*; `--dev` alone would use
    # the compiled-in genesis, which funds only two of the four actors.
    CFG_PRESET=dev \
    CHAINSPEC_GENESIS_STATE="$OUTDIR/genesis/genesis_state_${NETWORK}.mn" \
    CHAINSPEC_GENESIS_BLOCK="$OUTDIR/genesis/genesis_block_${NETWORK}.mn" \
    SIDECHAIN_BLOCK_BENEFICIARY=04bcf7ad3be7a5c790460be82a713af570f22e0f801f6659ab8e84a52be6969e \
    APPEND_ARGS="--chain= --rpc-port $RPC_PORT --port $((RPC_PORT + 1)) --prometheus-port $((RPC_PORT + 2))" \
        "$NODE_BIN" > "$OUTDIR/node.log" 2>&1 &
    NODE_PID=$!
    trap 'kill $NODE_PID 2>/dev/null || true' EXIT
    "$REPO_ROOT/local-environment/check-health.sh" -u "http://127.0.0.1:$RPC_PORT" -b 1 -t 120
}

verify_mock_verification_used() {
    grep -q 'mock-proofs build: contract and DUST proof verification is DISABLED' "$OUTDIR/node.log" \
        || { echo "  FAIL: node never took the mock-proofs verification path"; exit 1; }
    echo "  PASS: node verified the flow's transactions with proof checks skipped"
}

run_flow() {
    REUSE_ENV=1 IMAGE_TAG=unused NETWORK=$NETWORK PROOF_SERVER=mock OUTDIR="$OUTDIR/e2e" \
    NODE_WS="ws://127.0.0.1:$RPC_PORT" NODE_HTTP="http://127.0.0.1:$RPC_PORT" \
        "$REPO_ROOT/scripts/tests/microdao-local-env-e2e.sh"
}

echo "=== micro-dao e2e on a mock-proofs dev node ==="
rm -rf "$OUTDIR"
mkdir -p "$OUTDIR"
timed "Build node ($NODE_FEATURES) and toolkit" build
timed "Generate funded undeployed genesis"     generate_funded_genesis "$NETWORK" "$REPO_ROOT/res/dev" "$OUTDIR"
timed "Start dev node, wait for first block"   start_node
timed "Contract flow (see nested table)"       run_flow
timed "Verify mock verification path was used" verify_mock_verification_used
print_timings
