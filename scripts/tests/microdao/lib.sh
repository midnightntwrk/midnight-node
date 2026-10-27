# shellcheck shell=bash
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

# Shared by the micro-dao e2e scripts; source it, don't run it.

REPO_ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)
TOOLKIT_BIN=$REPO_ROOT/target/ci-e2e/midnight-node-toolkit

ACTORS=(organizer red blue green)
# One wallet per actor: a wallet's DUST spends chain strictly, so a shared wallet would allow
# only one transaction per block.
declare -A SEED=(
    [organizer]=0000000000000000000000000000000000000000000000000000000000000002
    [red]=0000000000000000000000000000000000000000000000000000000000000003
    [blue]=0000000000000000000000000000000000000000000000000000000000000004
    [green]=0000000000000000000000000000000000000000000000000000000000000005
)

TIMINGS=()
SCRIPT_START=$(date +%s)

timed() {
    local label=$1; shift
    echo ""
    echo "▶ $label"
    local start
    start=$(date +%s)
    "$@"
    local secs=$(( $(date +%s) - start ))
    TIMINGS+=("$(printf '%-48s %6ds' "$label" "$secs")")
    echo "  ✓ ${secs}s"
}

print_timings() {
    echo ""
    echo "============================================================"
    printf '%s\n' "${TIMINGS[@]}"
    printf '%-48s %6ds\n' "TOTAL" $(( $(date +%s) - SCRIPT_START ))
    echo "============================================================"
}

build_toolkit() {
    cargo build --profile ci-e2e -p midnight-node-toolkit --manifest-path "$REPO_ROOT/Cargo.toml"
}

genesis_seeds_json() {
    local actor
    for actor in "${ACTORS[@]}"; do echo "{\"$actor\": \"${SEED[$actor]}\"}"; done | jq -cs 'add'
}

# Usage: generate_funded_genesis <network> <res config dir> <output dir>
generate_funded_genesis() {
    local network=$1 config=$2 out=$3
    genesis_seeds_json > "$out/genesis-seeds.json"
    "$TOOLKIT_BIN" generate-genesis \
        --network "$network" \
        --seeds-file "$out/genesis-seeds.json" \
        --ledger-parameters-config "$config/ledger-parameters-config.json" \
        --cnight-generates-dust-config "$config/cnight-config.json" \
        --ics-config "$config/ics-config.json" \
        --reserve-config "$config/reserve-config.json" \
        -o "$out/genesis"
}
