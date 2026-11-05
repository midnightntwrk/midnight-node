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

# Table WAVE-<n>: the smoke after each binary wave, runtime still ledger 8. Leaves seed 3
# alone: it backs the unsent ledger-8 transaction.
#
#   wave_smoke.sh <wave>     WAVE_EXPECTED_L9: how many RPC nodes should run the new binary by now
set -e
source "$(dirname "${BASH_SOURCE[0]}")/../lib/suite.sh"

WAVE="${1:?usage: wave_smoke.sh <wave number>}"
EV="$EVIDENCE_DIR/wave$WAVE"; rm -rf "$EV"; mkdir -p "$EV"
use_toolkit l8
indexer_resolve || true
require_baseline
state_load "$PRE_FORK_STATE"
export DAPP_FILE_PREFIX

t_table "WAVE-$WAVE" "Wave $WAVE of the binary rollout on $NETWORK_NAME (runtime still ledger 8)"

c_spec() { local s; s=$(spec_version); [ "$s" = "$SPEC_BEFORE" ] && t_pass "spec $s" || t_fail "spec is $s before the runtime upgrade (expected $SPEC_BEFORE)"; }
c_versions() {
    local nv l9 total detail
    nv=$(node_versions); total=$(grep -c . <<< "$nv"); l9=$(grep -cF "=$L9_EXPECTED_NODE_PREFIX" <<< "$nv"); detail=" ${nv//$'\n'/ }"
    if [ "$TARGET" = network ] && [ -n "${KUBE_CONTEXT:-}" ]; then
        local pods; pods=$(_kubectl --request-timeout=15s get pods \
            -o jsonpath='{range .items[*]}{.metadata.name}={.spec.containers[0].image}{"\n"}{end}' 2>/dev/null | grep -E "${KUBE_POD_REGEX:-^midnight-[0-9]+-}")
        [ -n "$pods" ] && { echo "$pods" > "$EV/pods.txt"; detail="$detail; pods on $L9_EXPECTED_NODE_PREFIX: $(grep -c ":$L9_EXPECTED_NODE_PREFIX" "$EV/pods.txt")/$(wc -l < "$EV/pods.txt" | tr -d ' ')"; }
    fi
    if [ "$total" = 0 ]; then t_fail "no node answering"
    elif [ -n "${WAVE_EXPECTED_L9:-}" ] && [ "$l9" -lt "$WAVE_EXPECTED_L9" ]; then t_fail "$l9/$total RPC node(s) on $L9_EXPECTED_NODE_PREFIX, expected $WAVE_EXPECTED_L9:$detail"
    elif [ "$l9" = 0 ]; then t_warn "no RPC node on $L9_EXPECTED_NODE_PREFIX after the wave (RPC nodes may not be validators; WAVE_EXPECTED_L9 makes this a check):$detail"
    else t_pass "$l9/$total RPC node(s) on $L9_EXPECTED_NODE_PREFIX:$detail"; fi
}
c_agree() {
    multi_node || { t_skip "one RPC endpoint"; return; }
    local fin; fin=$(finalized_height)
    check_state_agreement "$(block_hash_at "$NODE_HTTP" "$fin")" > "$EV/agreement.txt" 2>&1 \
        && sweep_forks $(( fin > 20 ? fin - 20 : 1 )) "$fin" >> "$EV/agreement.txt" 2>&1 \
        && t_pass "state and block hashes agree up to finalized #$fin" || t_fail "$(grep -m1 -E 'FORK|DIVERGENCE|NO AGREEMENT' "$EV/agreement.txt")"
}
c_transfer() {
    tk_tx generate-txs single-tx --source-seed "$SEED_1" --unshielded-amount 1 --destination-address "$ADDR_U2" -d "$NODE_WS" \
        > "$EV/transfer.log" 2>&1 && t_pass "landed" || t_fail "$(last_line_of "$EV/transfer.log")"
}
c_contracts() {
    [ -n "${CONTRACT_A:-}" ] || { t_skip "no contract A"; return; }
    tk_tx generate-txs contract-simple call --funding-seed "$SEED_1" --call-key check --rng-seed "$RNG_A" \
        --contract-address "$CONTRACT_A" -d "$NODE_WS" > "$EV/contract_a.log" 2>&1 || { t_fail "check on A: $(last_line_of "$EV/contract_a.log")"; return; }
    local a bad=""
    for a in "$CONTRACT_A" ${CONTRACT_B:-}; do
        semantic_snapshot "$a" "$EV/state_$a.txt" && diff -q "$STATE_DIR/contract_state/$a.txt" "$EV/state_$a.txt" >/dev/null || bad="$bad $(registry_label "$a")"
    done
    [ -z "$bad" ] && t_pass "check on A landed; A and B unchanged" || t_fail "semantic state changed or unreadable:$bad"
}
c_dapp() {
    [ -n "${DAPP_COUNTER:-}" ] || { t_skip "no counter dApp"; return; }
    dapp_call counter "$SEED_1" "$DAPP_COUNTER" increment && t_pass "counter.increment landed" || t_fail "$(dapp_tail counter increment_send)"
}
c_indexer() {
    [ -n "$INDEXER_GQL" ] || { t_fail "no indexer answering"; return; }
    local r; r=$(indexer_catch_up 180) || { t_fail "$r"; return; }
    cp "$(dapp_log counter increment_send)" "$EV/dapp_send.log" 2>/dev/null || true
    r=$(tx_inclusion_check "$EV/tx_inclusion.tsv" "$EV"/*.log) && t_pass "at the tip; $r" || t_fail "$r"
}
t_check WAVE-1 "HF-13" "runtime spec unchanged ($SPEC_BEFORE)" c_spec
t_check WAVE-2 "HF-13" "blocks produced and finalized over 60 s" c_health
t_check WAVE-3 "HF-13" "node versions after the wave" c_versions
t_check WAVE-4 "HF-13" "cross-node agreement at the finalized block" c_agree
t_check WAVE-5 "HF-13" "an unshielded transfer seed 1 -> seed 2 lands" c_transfer
t_check WAVE-6 "HF-13" "a call on contract A lands; A and B semantically unchanged" c_contracts
t_check WAVE-7 "HF-13" "counter.increment on the pre-fork counter lands" c_dapp
t_check WAVE-8 "HF-13, HF-12" "the indexer is at the tip and holds every transaction of this smoke" c_indexer
t_finish
