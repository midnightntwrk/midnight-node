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

# Table PRE: are the network, this machine and the seeds ready? Read-only, apart from
# warming both toolkit caches: hours the first time on a long chain, a delta after that.
set -e
export TARGET=network
source "$(dirname "${BASH_SOURCE[0]}")/../lib/suite.sh"
EV="$EVIDENCE_DIR/preflight"; mkdir -p "$EV"

t_table PRE "Preflight on $NETWORK_NAME ($NODE_HTTP)"

c_rpc() {
    local chain s v; chain=$(rpc system_chain | jq -r '.result // empty'); s=$(spec_version); v=$(node_version)
    [ -n "$s" ] && t_pass "$chain: node $v, spec $s, height $(node_height), finalized $(finalized_height)" || t_fail "no answer from $NODE_HTTP"
}
c_indexer() {
    indexer_resolve || { t_fail "no GraphQL path of $INDEXER_BASE answers ($INDEXER_GQL_PATHS)"; return; }
    local r; r=$(indexer_catch_up 120) && t_pass "$INDEXER_GQL at #$(indexer_height)" || t_warn "$r"
}
# A toolkit cache holds one chain, so a network reset wipes it.
c_genesis() {
    local g era m; g=$(genesis_hash)
    [ -n "$g" ] || { t_fail "no genesis hash"; return; }
    if [ -n "${EXPECTED_GENESIS_HASH:-}" ] && [ "$g" != "$EXPECTED_GENESIS_HASH" ]; then t_fail "genesis $g, the env expects $EXPECTED_GENESIS_HASH"; return; fi
    for era in l8 l9; do
        local d="$CACHE_DIR/toolkit-$era"; mkdir -p "$d"; m=$(cat "$d/.genesis" 2>/dev/null)
        if [ -n "$m" ] && [ "$m" != "$g" ]; then rm -rf "$d"; mkdir -p "$d"; t_info "$era cache belonged to genesis $m: wiped"; fi
        echo "$g" > "$d/.genesis"
    done
    t_pass "genesis $g"
}
c_images() {
    local img bad=""
    for img in "$L8_TOOLKIT_IMAGE" "$(l9_toolkit_image)"; do require_image "$img" 2>/dev/null || bad="$bad $img"; done
    [ -z "$bad" ] && t_pass "$L8_TOOLKIT_IMAGE, $(l9_toolkit_image)" || t_fail "missing:$bad"
}
c_warm() {  # <era>
    use_toolkit "$1"
    local args=() a
    for a in "$SEED_1" "$SEED_2" "$SEED_3" "$SEED_4"; do args+=(--seeds "$a"); done
    for a in $(seq 1 "${WARM_ATTEMPTS:-12}"); do
        tk_chain fetch "${args[@]}" > "$EV/warm_$1.log" 2>&1 && { t_pass "cache at $TK_CACHE_DIR, height $(node_height) (attempt $a)"; return; }
        sleep 10
    done
    t_fail "$(last_line_of "$EV/warm_$1.log")"
}
c_clients() {
    proof_server_up l8 > "$EV/ps_l8.log" 2>&1 && proof_server_up l9 > "$EV/ps_l9.log" 2>&1 || { t_fail "proof servers: $(last_line_of "$EV/ps_l9.log")"; return; }
    clients_ready > "$EV/clients.log" 2>&1 || { t_fail "npm ci in clients/"; return; }
    t_pass "proof servers $(proof_server_version "$PS_L8_URL") at $PS_L8_URL, $(proof_server_version "$PS_L9_URL") at $PS_L9_URL; clients $(clients_versions | cut -d' ' -f1-2)"
}
t_check PRE-1 "-" "node RPC answers" c_rpc
t_check PRE-2 "-" "indexer answers and follows the tip" c_indexer
t_check PRE-3 "-" "genesis recorded; toolkit caches belong to it" c_genesis
t_check PRE-4 "-" "both toolkit images available" c_images
t_check PRE-5 "-" "ledger-8 toolkit cache warm for the four test wallets" c_warm l8
t_check PRE-6 "HF-16" "ledger-9 toolkit cache warm for the four test wallets" c_warm l9
t_check PRE-7 "-" "proof servers and the client workspace ready" c_clients
t_finish
