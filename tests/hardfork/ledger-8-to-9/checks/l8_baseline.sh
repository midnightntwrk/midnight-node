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

# Table L8: the ledger-8 baseline. Creates the state the fork must preserve and records it.
set -e
source "$(dirname "${BASH_SOURCE[0]}")/../lib/suite.sh"

EV="$EVIDENCE_DIR/l8"; mkdir -p "$EV" "$STATE_DIR/contract_state"
use_toolkit l8
require_indexer
require_era l8

state_load "$PRE_FORK_STATE"
: "${DAPP_FILE_PREFIX:=$([ "$TARGET" = local ] && echo local || echo "b$(date -u '+%Y%m%dT%H%M')")}"
export DAPP_FILE_PREFIX
st() { state_set "$PRE_FORK_STATE" "$1" "$2"; }
st DAPP_FILE_PREFIX "$DAPP_FILE_PREFIX"
addr() { printf -v "$1" '%s' "$(tk_address "$2" "$3")"; st "$1" "${!1}"; }
for i in 1 2 3 4; do s="SEED_$i"; addr "ADDR_U$i" "${!s}" --unshielded; addr "ADDR_S$i" "${!s}" --shielded; done
dapp_var() { echo "DAPP_$(echo "$1" | tr 'a-z-' 'A-Z_')"; }

t_table L8 "Ledger-8 baseline on $NETWORK_NAME (node $(node_version), spec $SPEC_NOW, toolkit $TK_IMAGE)"

t_section "versions"
c_ledger() { local v; v=$(ledger_version); st LEDGER_VER_BEFORE "$v"
    [ "$v" = "$L8_EXPECTED_LEDGER" ] && t_pass "$v" || t_fail "reported '$v'"; }
c_spec() { st SPEC_BEFORE "$SPEC_NOW"; st NODE_VER_BEFORE "$(node_version)"
    [ "$SPEC_NOW" = "$L8_EXPECTED_SPEC" ] && t_pass "spec $SPEC_NOW" || t_fail "spec $SPEC_NOW"; }
c_toolkit() { local v; v=$(tk_version); echo "$v" > "$EV/toolkit_version.txt"
    [[ "$v" == *"Node: $L8_EXPECTED_NODE_PREFIX"* && "$v" == *"Compactc: $L8_EXPECTED_COMPACTC"* ]] && t_pass "$v" || t_fail "$v"; }
c_genesis() { local g; g=$(genesis_hash); st GENESIS_HASH "$g"
    if [ -n "${EXPECTED_GENESIS_HASH:-}" ] && [ "$g" != "$EXPECTED_GENESIS_HASH" ]; then t_fail "genesis $g, the env expects $EXPECTED_GENESIS_HASH (network reset?)"
    else t_pass "genesis $g"; fi; }
t_check L8-VER-1 "HF-01" "midnight_ledgerVersion is $L8_EXPECTED_LEDGER" c_ledger
t_check L8-VER-2 "HF-01" "runtime spec is $L8_EXPECTED_SPEC" c_spec
t_check L8-VER-3 "HF-01" "the ledger-8 toolkit is node $L8_EXPECTED_NODE_PREFIX with compactc $L8_EXPECTED_COMPACTC" c_toolkit
t_check L8-VER-4 "-" "genesis hash recorded, and matches the network env if it names one" c_genesis

t_check L8-NET-1 "HF-01" "blocks are produced and finalized over 60 s" c_health 7

t_section "funds and DUST"
c_funds() {
    local i s u sh dt ds problems="" detail=""
    printf 'seed\tunshielded_night\tshielded_night\tdust_total\tdust_sources\n' > "$EV/funds.tsv"
    for i in 1 2 3 4; do
        s="SEED_$i"
        wallet_snapshot "${!s}" "$EV/wallet_seed$i.json" || { problems="$problems; seed $i show-wallet failed"; continue; }
        dust_snapshot "${!s}" "$EV/dust_seed$i.json" || { problems="$problems; seed $i dust-balance failed"; continue; }
        u=$(wallet_night_unshielded "$EV/wallet_seed$i.json"); sh=$(wallet_night_shielded "$EV/wallet_seed$i.json")
        dt=$(jq -r .total "$EV/dust_seed$i.json"); ds=$(jq -r '.source | length' "$EV/dust_seed$i.json")
        printf '%s\t%s\t%s\t%s\t%s\n' "$i" "$u" "$sh" "$dt" "$ds" >> "$EV/funds.tsv"
        detail="$detail seed$i=${u}N/${ds}src"
        if [ "$i" -le 3 ]; then
            big_gt "$u" 0 || problems="$problems; seed $i has no unshielded NIGHT"
            big_gt "$dt" 0 || problems="$problems; seed $i has no DUST to pay fees"
        fi
    done
    [ -z "$problems" ] && t_pass "$detail" || t_fail "${problems#; } ($detail)"
}
t_check L8-FUND-1 "HF-04, HF-05" "seeds 1-3 hold NIGHT and DUST; balances of all four recorded" c_funds

c_dust_present() {
    [ -s "$EV/dust_seed1.json" ] || dust_snapshot "$SEED_1" "$EV/dust_seed1.json" || { t_fail "dust-balance unreadable"; return; }
    local src tot; src=$(jq '.source | length' "$EV/dust_seed1.json"); tot=$(jq -r .total "$EV/dust_seed1.json")
    [ "$src" -gt 0 ] && big_gt "$tot" 0 && t_pass "$src sources, total $tot" || t_fail "$src sources, total $tot"
}
c_dust_growth() { local g; g=$(dust_growth_check "$SEED_1" "$EV/dust_growth") && t_pass "$g" || t_fail "no source grew: $g"; }
t_check L8-DUST-1 "HF-04" "seed 1 has DUST and a non-empty generation source set" c_dust_present
t_check L8-DUST-2 "HF-04" "DUST generation is active (a below-cap source grows)" c_dust_growth

t_section "transfers"
t_check L8-TX-1 "HF-05" "unshielded NIGHT transfer seed 1 -> seed 2" c_tx tx_unshielded \
    --source-seed "$SEED_1" --unshielded-amount 10 --destination-address "$ADDR_U2"
t_check L8-TX-2 "HF-05" "shielded NIGHT transfer seed 1 -> seed 2" c_tx tx_shielded \
    --source-seed "$SEED_1" --shielded-amount 10 --destination-address "$ADDR_S2"
t_check L8-TX-3 "HF-05" "one transaction with shielded and unshielded outputs, seed 1 -> seed 3" c_tx tx_mixed \
    --source-seed "$SEED_1" --shielded-amount 5 --unshielded-amount 5 --destination-address "$ADDR_S3" --destination-address "$ADDR_U3"
t_check L8-TX-4 "HF-05" "one unshielded transfer to two destinations, seed 1 -> seeds 3 and 4" c_tx tx_multi_dest \
    --source-seed "$SEED_1" --unshielded-amount 5 --destination-address "$ADDR_U3" --destination-address "$ADDR_U4"
t_check L8-TX-5 "HF-05" "unshielded transfer from the second wallet, seed 2 -> seed 1" c_tx tx_second_wallet \
    --source-seed "$SEED_2" --unshielded-amount 10 --destination-address "$ADDR_U1"

t_section "built-in contracts"
cs_call() {  # <log> <seed> <rng> <address> <key>
    tk_tx generate-txs contract-simple call --funding-seed "$2" --call-key "$5" --rng-seed "$3" \
        --contract-address "$4" -d "$NODE_WS" >> "$EV/$1.log" 2>&1
}
c_cs() {  # <A|B> <seed> [deploy args...]
    local label=$1 seed=$2 rng addr l="${1,,}" f="$EV/deploy_${1,,}.mn"; shift 2
    rng=$(openssl rand -hex 32)
    tk_chain generate-txs --dest-file "$f" contract-simple deploy --funding-seed "$seed" --rng-seed "$rng" "$@" > "$EV/cs_$l.log" 2>&1 \
        && tk_send "$f" >> "$EV/cs_$l.log" 2>&1 && addr=$(tk contract-address --src-file "$f" 2>/dev/null | tail -1) \
        && is_hex64 "$addr" || { t_fail "deploy: $(last_line_of "$EV/cs_$l.log")"; return; }
    printf -v "CONTRACT_$label" '%s' "$addr"; printf -v "RNG_$label" '%s' "$rng"
    st "CONTRACT_$label" "$addr"; st "RNG_$label" "$rng"; registry_add "$label" contract-simple "$addr" l8
    cs_call "cs_$l" "$seed" "$rng" "$addr" store && cs_call "cs_$l" "$seed" "$rng" "$addr" check \
        && t_pass "$label = $addr; store, check landed" || t_fail "$label = $addr; $(last_line_of "$EV/cs_$l.log")"
}
c_cs_rotate() {
    [ -n "${CONTRACT_B:-}" ] || { t_skip "no contract B"; return; }
    tk_tx generate-txs contract-simple maintenance --funding-seed "$SEED_2" --authority-seed "$SEED_1" \
        --new-authority-seed "$SEED_2" --new-authority-seed "$SEED_3" --threshold 1 --counter 0 \
        --contract-address "$CONTRACT_B" --rng-seed "$RNG_B" -d "$NODE_WS" > "$EV/cs_b_rotate.log" 2>&1 \
        || { t_fail "$(last_line_of "$EV/cs_b_rotate.log")"; return; }
    semantic_snapshot "$CONTRACT_B" "$EV/cs_b_rotated.txt" || { t_fail "state unreadable after the rotation"; return; }
    if grep -qx 'authority_counter=1' "$EV/cs_b_rotated.txt"; then st CONTRACT_B_COUNTER 1; t_pass "committee is seeds 2+3, counter 0 -> 1"
    else t_fail "authority counter is $(grep authority_counter "$EV/cs_b_rotated.txt")"; fi
}
record_states() {  # <address>...
    local a ok=0
    for a in "$@"; do semantic_snapshot "$a" "$STATE_DIR/contract_state/$a.txt" && ok=$((ok + 1)); done
    [ "$#" -gt 0 ] && [ "$ok" = "$#" ] && t_pass "$ok recorded in state/contract_state/" || t_fail "$ok of $# recorded"
}
c_cs_record() { record_states ${CONTRACT_A:-} ${CONTRACT_B:-}; }
t_check L8-CS-1 "HF-02" "contract A (committee seed 1): deploy, store, check" c_cs A "$SEED_1"
t_check L8-CS-2 "HF-02, HF-03" "contract B (committee seeds 1+2, threshold 1): deploy, store, check" \
    c_cs B "$SEED_2" --authority-seed "$SEED_1" --authority-seed "$SEED_2" --authority-threshold 1
t_check L8-CS-3 "HF-03" "contract B's committee rotated on ledger 8 (seed 1 signs; new committee seeds 2+3)" c_cs_rotate
t_check L8-CS-4 "HF-02" "semantic state of A and B recorded (entry points, committee, counter)" c_cs_record

t_section "Compact dApps"
c_dapp_deploy() {
    local d addr ok="" failed=""
    for d in "${DAPPS_PRE[@]}" "${DAPPS_PRE_BEST_EFFORT[@]}"; do
        addr=$(dapp_deploy "$d" "$SEED_1" || true)
        if is_hex64 "$addr"; then
            ok="$ok $d"; st "$(dapp_var "$d")" "$addr"; registry_add "$d" compact "$addr" l8
            t_info "$d at $addr (compactc $(cat "$(dapp_dir "$d")/compactc.out.version"))"
        elif [[ " ${DAPPS_PRE[*]} " == *" $d "* ]]; then failed="$failed $d($addr)"
        else t_info "$d not deployed ($addr), best effort"; fi
    done
    state_load "$PRE_FORK_STATE"
    [ -z "$failed" ] && t_pass "deployed:$ok" || t_fail "failed:$failed; deployed:$ok"
}
c_dapp_calls() {
    [ -n "${DAPP_COUNTER:-}" ] && [ -n "${DAPP_BBOARD:-}" ] || { t_skip "counter or bboard not deployed"; return; }
    dapp_call counter "$SEED_1" "$DAPP_COUNTER" increment && dapp_call counter "$SEED_1" "$DAPP_COUNTER" increment \
        || { t_fail "counter.increment: $(dapp_tail counter increment_send)"; return; }
    dapp_call bboard "$SEED_1" "$DAPP_BBOARD" post "posted-before-the-fork" \
        || { t_fail "bboard.post: $(dapp_tail bboard post_send)"; return; }
    t_pass "counter.increment x2 (private count $(jq -r .count "$(dapp_file counter_private_state.json)" 2>/dev/null)), bboard.post landed"
}
c_dapp_record() {
    local d v a=()
    for d in "${DAPPS_PRE[@]}" "${DAPPS_PRE_BEST_EFFORT[@]}"; do v=$(dapp_var "$d"); [ -z "${!v:-}" ] || a+=("${!v}"); done
    record_states "${a[@]}"
}
t_check L8-DAPP-1 "HF-02" "counter and bboard deployed with compactc $L8_EXPECTED_COMPACTC (micro-dao best effort)" c_dapp_deploy
t_check L8-DAPP-2 "HF-02" "counter.increment x2 and bboard.post (poster derived from private state)" c_dapp_calls
t_check L8-DAPP-3 "HF-02" "semantic state of every dApp recorded" c_dapp_record

c_params() {
    tk show-ledger-parameters -r "$NODE_WS" > "$EV/params.txt" 2>&1 && grep -q overall_price "$EV/params.txt" \
        && { cp "$EV/params.txt" "$STATE_DIR/params_l8.txt"; t_pass "$(grep -c ':' "$EV/params.txt") lines (evidence/l8/params.txt)"; } \
        || t_fail "$(last_line_of "$EV/params.txt")"
}
t_check L8-PARAM-1 "HF-09" "ledger parameters recorded" c_params

c_fetch() {
    fetch_all "$EV/fetch.log" && t_pass "fetched to height $(node_height)" || t_fail "$(last_line_of "$EV/fetch.log")"
}
t_check L8-FETCH-1 "HF-16" "every block fetched and decoded by the ledger-8 toolkit" c_fetch

t_section "indexer"
c_idx_sync() { local r; r=$(indexer_catch_up 180) && t_pass "indexer $(indexer_height), node finalized $(finalized_height)" || t_fail "$r"; }
c_idx_contracts() {
    local a t missing="" n=0
    for a in $(registry_addresses); do
        n=$((n + 1)); t=$(indexer_contract_type "$a"); [ -n "$t" ] || missing="$missing $(registry_label "$a")"
    done
    [ -z "$missing" ] && [ "$n" -gt 0 ] && t_pass "$n contracts indexed" || t_fail "not indexed:${missing:- none recorded}"
}
c_idx_tip() {
    local b pv dge
    b=$(indexer_block); pv=$(echo "$b" | jq -r '.protocolVersion // empty'); dge=$(echo "$b" | jq -r '.dustGenerationEndIndex // empty')
    st PRE_PROTOCOL_VERSION "$pv"; st PRE_DUST_GEN_END "$dge"
    if [ -z "$pv" ]; then t_fail "block query incomplete: $b"
    elif [ -z "$dge" ] || [ "$dge" -le 0 ]; then t_fail "dustGenerationEndIndex '$dge': the fork's DUST reset would not be observable"
    else t_pass "protocolVersion $pv, dustGenerationEndIndex $dge"; fi
}
c_idx_inclusion() {
    local r; r=$(tx_inclusion_check "$EV/tx_inclusion.tsv" "$EV"/*.log "$DAPPS_DIR"/*/"${DAPP_FILE_PREFIX}"_*send.log) \
        && t_pass "$r" || t_fail "$r (evidence/l8/tx_inclusion.tsv)"
}
t_check L8-IDX-1 "HF-12" "the indexer follows the finalized tip" c_idx_sync
t_check L8-IDX-2 "HF-12" "every contract deployed above is indexed" c_idx_contracts
t_check L8-IDX-3 "HF-12" "the indexer tip reports protocolVersion and a non-empty DUST generation tree" c_idx_tip
t_check L8-IDX-4 "HF-12" "every transaction of this phase is in a block, looked up by hash on the indexer" c_idx_inclusion

c_eco() {
    [ "$TARGET" = network ] || { t_skip "no faucet or explorer on local-env"; return; }
    local u c out="" bad=""
    for u in ${FAUCET_URL:-} ${EXPLORER_URL:-}; do
        c=$(curl -s -m 15 -o /dev/null -w '%{http_code}' "$u"); out="$out $u=$c"; [ "$c" = 200 ] || bad=1
    done
    [ -z "$out" ] && { t_skip "no FAUCET_URL / EXPLORER_URL in the network env"; return; }
    [ -z "$bad" ] && t_pass "$out" || t_warn "$out"
}
t_check L8-ECO-1 "-" "faucet and explorer answer" c_eco

t_section "fork preparation"
c_v8_tx() {
    local f="$STATE_DIR/v8_tx.mn" xt v
    rm -f "$STATE_DIR/v8_tx.xt"
    tk_chain generate-txs --dest-file "$f" single-tx --source-seed "$SEED_3" --unshielded-amount 3 \
        --destination-address "$ADDR_U2" > "$EV/v8_tx.log" 2>&1 && [ -s "$f" ] || { t_fail "$(last_line_of "$EV/v8_tx.log")"; return; }
    xt=$(bare_extrinsic "$f") || { t_fail "no Midnight transaction in $f"; return; }
    v=$(validate_extrinsic "$xt"); echo "$v" > "$EV/v8_tx_validate.txt"
    [[ "$v" == 0x00* ]] || { t_fail "the node does not accept it on ledger 8: validate_transaction $v"; return; }
    echo "$xt" > "$STATE_DIR/v8_tx.xt"
    wallet_snapshot "$SEED_3" "$EV/seed3_hf_pre.json" || { t_fail "seed 3 unreadable"; return; }
    SEED3_NIGHT=$(wallet_night "$EV/seed3_hf_pre.json"); st SEED3_NIGHT "$SEED3_NIGHT"
    t_pass "the node validates it (${v:0:12}...); saved unsent as state/v8_tx.xt; seed 3 holds $SEED3_NIGHT NIGHT (unshielded, shielded) and must not spend it before the fork"
}
t_check HF-PRE-1 "HF-08" "a ledger-8 transaction from seed 3, valid at the node, saved unsent as a bare extrinsic" c_v8_tx

use_toolkit l9
c_l9_replay() {
    require_image "$TK_IMAGE" 2>/dev/null || { t_fail "image $TK_IMAGE missing"; return; }
    if tk_chain_ro -q show-wallet --seed "$SEED_1" > "$EV/l9_toolkit_replay.log" 2>&1; then
        t_pass "$TK_IMAGE replayed $(node_height) blocks of runtime $SPEC_NOW"
    else t_fail "$(grep -m1 -o 'UnsupportedBlockVersion([0-9]*)' "$EV/l9_toolkit_replay.log" || last_line_of "$EV/l9_toolkit_replay.log")"; fi
}
c_ecdsa_refused() {
    if tk_chain generate-txs single-tx --source-seed "ecdsa:$SEED_1" --unshielded-amount 1 \
            --destination-address "$ADDR_U1" -d "$NODE_WS" > "$EV/ecdsa_prefork.log" 2>&1; then
        t_fail "an ECDSA-signed transfer was accepted on ledger 8"
    elif grep -qiE 'ledger 9|EcdsaNotSupportedForLedger' "$EV/ecdsa_prefork.log"; then
        t_pass "the toolkit refused: $(grep -m1 -ioE 'EcdsaNotSupportedForLedger\([A-Za-z0-9]*\)|[^"]{0,60}ledger 9[^"]{0,40}' "$EV/ecdsa_prefork.log")"
    else t_warn "refused, but not by the ledger-version gate: $(last_line_of "$EV/ecdsa_prefork.log")"; fi
}
t_check HF-PRE-2 "HF-01, node#2161" "the ledger-9 toolkit replays this ledger-8 chain (on a network this also fills its cache)" c_l9_replay
# Client-side only: ledger 8's wire format has no ECDSA signature, so the node would refuse
# any such transaction while decoding it, as L9-HF08-1 shows for the reverse direction.
t_check HF-PRE-3 "HF-06" "the ledger-9 toolkit refuses to build an ECDSA-signed NIGHT transfer while the chain is on ledger 8" c_ecdsa_refused

t_finish
