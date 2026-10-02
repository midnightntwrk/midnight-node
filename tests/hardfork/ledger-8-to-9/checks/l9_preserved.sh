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

# Table L9: what the fork preserved. Start a few minutes after the fork: nothing pays fees
# until the wallets re-register for DUST, and that needs DUST accrued since the fork.
set -e
source "$(dirname "${BASH_SOURCE[0]}")/../lib/suite.sh"

EV="$EVIDENCE_DIR/l9"; mkdir -p "$EV"
use_toolkit l9
require_indexer
require_baseline
state_load "$PRE_FORK_STATE"; state_load "$FORK_STATE"
export DAPP_FILE_PREFIX
require_era l9
FORK_HEIGHT=$(fork_height "$SPEC_BEFORE") || cannot_run "no verifiable fork block for spec $SPEC_BEFORE (the RPC node must keep old runtime state)"
st() { state_set "$PRE_FORK_STATE" "$1" "$2"; }
# Before any transfer of this table reaches seed 3 (L9-HF08-1 judges it).
wallet_snapshot "$SEED_3" "$EV/seed3_after_fork.json" || true

t_table L9 "Ledger 9 after the fork on $NETWORK_NAME: what the fork preserved (fork block #$FORK_HEIGHT, toolkit $TK_IMAGE)"

t_section "versions"
c_ledger() { local v; v=$(ledger_version); [[ "$v" == *"$L9_EXPECTED_LEDGER_SUBSTR"* ]] && t_pass "$v" || t_fail "reported '$v'"; }
c_spec() { [ "$SPEC_NOW" = "$L9_EXPECTED_SPEC" ] && t_pass "spec $SPEC_BEFORE -> $SPEC_NOW" || t_fail "spec $SPEC_NOW, expected $L9_EXPECTED_SPEC"; }
c_nodes() {
    local nv bad
    nv=$(node_versions); bad=$(grep -vF "=$L9_EXPECTED_NODE_PREFIX" <<< "$nv" | cut -d= -f1 | xargs)
    [ -n "$nv" ] && [ -z "$bad" ] && t_pass "${nv//$'\n'/ }" || t_fail "not on $L9_EXPECTED_NODE_PREFIX: ${bad:-no node answering} (${nv//$'\n'/ })"
}
c_toolkit() {
    local v; v=$(tk_version); echo "$v" > "$EV/toolkit_version.txt"
    [[ "$v" == *"Node: $L9_EXPECTED_NODE_PREFIX"* && "$v" == *"$L9_EXPECTED_LEDGER_TAG"* && "$v" == *"Compactc: $L9_EXPECTED_COMPACTC"* ]] \
        && t_pass "$v" || t_fail "$v"
}
t_check L9-VER-1 "HF-01" "midnight_ledgerVersion is a ledger-9 build" c_ledger
t_check L9-VER-2 "HF-01" "runtime spec is $L9_EXPECTED_SPEC" c_spec
t_check L9-VER-3 "HF-01" "every reachable node runs $L9_EXPECTED_NODE_PREFIX" c_nodes
t_check L9-VER-4 "HF-01" "the ledger-9 toolkit is node $L9_EXPECTED_NODE_PREFIX, $L9_EXPECTED_LEDGER_TAG, compactc $L9_EXPECTED_COMPACTC" c_toolkit
t_check L9-NET-1 "HF-01" "blocks are produced and finalized over 60 s" c_health

t_section "DUST across the fork"
c_dust_replay() {
    logs_available || { t_skip "no node logs on this target (KUBE_CONTEXT reads them)"; return; }
    local n line applied missing="" detail=""
    for n in $(log_nodes); do
        line=$(node_logs "$n" | grep -oE 'DustReapplyCompleted: dust generation replay complete, [0-9]+ applied, [0-9]+ skipped' | tail -1)
        if [ -z "$line" ] || ! node_logs "$n" | grep -q DustReapplyStarted; then missing="$missing $n"; continue; fi
        applied=$(echo "$line" | grep -oE '[0-9]+ applied' | grep -oE '[0-9]+')
        detail="$detail $n=$applied/$(echo "$line" | grep -oE '[0-9]+ skipped' | grep -oE '[0-9]+')"
    done
    [ -z "$missing" ] && t_pass "applied/skipped per node:$detail" || t_fail "no DUST replay logged on:$missing"
}
c_dust_reset() {
    local i s bad="" detail=""
    for i in 1 2 3 4; do
        s="SEED_$i"; [ "$i" = "${CNIGHT_SEED_INDEX:-0}" ] && continue
        dust_snapshot "${!s}" "$EV/dust_after_fork_seed$i.json" || { bad="$bad seed$i(unreadable)"; continue; }
        detail="$detail seed$i=$(jq '.source | length' "$EV/dust_after_fork_seed$i.json")/$(jq -r .total "$EV/dust_after_fork_seed$i.json")"
        [ "$(jq -r .total "$EV/dust_after_fork_seed$i.json")" = 0 ] || bad="$bad seed$i"
    done
    [ -z "$bad" ] && t_pass "sources/total:$detail" || t_fail "not reset:$bad ($detail)"
}
c_dust_cnight() {
    [ -n "${CNIGHT_SEED_INDEX:-}" ] || { t_skip "no cNIGHT-backed test wallet (CNIGHT_SEED_INDEX); local-env observes no cNIGHT"; return; }
    local s="SEED_$CNIGHT_SEED_INDEX" f="$EV/dust_after_fork_cnight.json" src
    dust_snapshot "${!s}" "$f" || { t_fail "dust-balance unreadable"; return; }
    src=$(jq '.source | length' "$f")
    [ "$src" -ge 1 ] && big_gt "$(jq -r .total "$f")" 0 && t_pass "seed $CNIGHT_SEED_INDEX: $src cNIGHT-backed source(s), total $(jq -r .total "$f")" \
        || t_fail "seed $CNIGHT_SEED_INDEX: no replayed cNIGHT source"
}
c_dust_register() {
    local i s bad=""
    for i in 1 2 3; do
        s="SEED_$i"
        tk_tx generate-txs register-dust-address --wallet-seed "${!s}" -d "$NODE_WS" > "$EV/dust_register_seed$i.log" 2>&1 \
            || bad="$bad seed$i: $(last_line_of "$EV/dust_register_seed$i.log" 120);"
    done
    [ -z "$bad" ] && t_pass "seeds 1-3 re-registered, fees paid from DUST accrued since the fork" || t_fail "$bad"
}
c_dust_restart() {
    wait_blocks 3
    local g; g=$(dust_growth_check "$SEED_1" "$EV/dust_restart") || { t_fail "no source grew: $g"; return; }
    t_pass "$g; total $(jq -r .total "$EV/dust_restart_s2.json") (before the fork: $(jq -r .total "$EVIDENCE_DIR/l8/dust_seed1.json" 2>/dev/null))"
}
t_check L9-DUST-1 "HF-04, node#2012" "the cNIGHT DUST replay migration ran on every node (DustReapplyStarted / Completed)" c_dust_replay
t_check L9-DUST-2 "HF-04, node#2012" "native DUST reset to zero by design for every test wallet" c_dust_reset
t_check L9-DUST-3 "HF-04, node#2012" "the cNIGHT-backed wallet kept a replayed DUST source" c_dust_cnight
t_check L9-DUST-4 "HF-04" "self-funded DUST re-registration for seeds 1-3" c_dust_register
t_check L9-DUST-5 "HF-04" "DUST generation restarts after re-registration" c_dust_restart

t_section "pre-fork contracts"
cs_call() {  # <log> <seed> <rng> <address> <keys...>
    local log="$EV/$1.log" seed=$2 rng=$3 addr=$4 k; shift 4
    for k in "$@"; do
        tk_tx generate-txs contract-simple call --funding-seed "$seed" --call-key "$k" --rng-seed "$rng" \
            --contract-address "$addr" -d "$NODE_WS" >> "$log" 2>&1 || { echo "$k: $(last_line_of "$log")"; return 1; }
    done
}
c_state() {  # <A|B> <seed>
    local a="CONTRACT_$1" rng="RNG_$1" r
    [ -n "${!a:-}" ] || { t_skip "no contract $1"; return; }
    r=$(cs_call "cs_${1,,}" "$2" "${!rng}" "${!a}" check store check) && t_pass "check, store, check on ${!a}" || t_fail "$r"
}
c_state_preserved() {
    local a r n=0 bad="" data=""
    for a in $(registry_addresses l8); do
        n=$((n + 1))
        r=$(contract_data_across_fork "$a" "$FORK_HEIGHT" "$EV/data_$a") && data="$data $(registry_label "$a")" || bad="$bad $(registry_label "$a")(data: $r)"
        [ -s "$STATE_DIR/contract_state/$a.txt" ] || continue
        semantic_snapshot "$a" "$EV/state_$a.txt" && diff "$STATE_DIR/contract_state/$a.txt" "$EV/state_$a.txt" > "$EV/state_$a.diff" \
            || bad="$bad $(registry_label "$a")(entry points or authority, evidence/l9/state_$a.diff)"
    done
    [ "$n" -gt 0 ] || { t_skip "no pre-fork contracts in the registry"; return; }
    [ -z "$bad" ] && t_pass "$n contracts: data identical in blocks #$((FORK_HEIGHT - 1)) and #$FORK_HEIGHT, entry points and authority equal to the ledger-8 record" || t_fail "$bad"
}
c_state_rotate() {
    [ -n "${CONTRACT_B:-}" ] || { t_skip "no contract B"; return; }
    local ctr="${CONTRACT_B_COUNTER:-1}"
    tk_tx generate-txs contract-simple maintenance --funding-seed "$SEED_2" --authority-seed "$SEED_2" \
        --new-authority-seed "$SEED_1" --new-authority-seed "$SEED_2" --threshold 1 --counter "$ctr" \
        --contract-address "$CONTRACT_B" --rng-seed "$RNG_B" -d "$NODE_WS" > "$EV/cs_b_rotate.log" 2>&1 \
        || { t_fail "$(last_line_of "$EV/cs_b_rotate.log")"; return; }
    semantic_snapshot "$CONTRACT_B" "$EV/cs_b_rotated.txt" || { t_fail "state unreadable after the rotation"; return; }
    grep -qx "authority_counter=$((ctr + 1))" "$EV/cs_b_rotated.txt" \
        && { st CONTRACT_B_COUNTER "$((ctr + 1))"; t_pass "seed 2 signed; committee seeds 1+2; counter $ctr -> $((ctr + 1))"; } \
        || t_fail "$(grep authority_counter "$EV/cs_b_rotated.txt")"
}
t_check L9-STATE-1 "HF-02, HF-07" "contract A: check (state survived), store, check" c_state A "$SEED_1"
t_check L9-STATE-2 "HF-02, HF-07" "contract B: check, store, check" c_state B "$SEED_2"
t_check L9-STATE-3 "HF-02" "every pre-fork contract: its data unchanged across the fork block; entry points and authority as recorded on ledger 8" c_state_preserved
t_check L9-STATE-4 "HF-03" "contract B's committee rotated on ledger 9 with the recorded counter" c_state_rotate

t_section "transfers from pre-fork funds"
t_check L9-TX-1 "HF-05" "unshielded NIGHT transfer seed 1 -> seed 3" c_tx tx_unshielded \
    --source-seed "$SEED_1" --unshielded-amount 10 --destination-address "$ADDR_U3"
t_check L9-TX-2 "HF-05" "shielded NIGHT transfer seed 1 -> seed 3" c_tx tx_shielded \
    --source-seed "$SEED_1" --shielded-amount 10 --destination-address "$ADDR_S3"
t_check L9-TX-3 "HF-05" "one transaction with shielded and unshielded outputs, seed 2 -> seed 3" c_tx tx_mixed \
    --source-seed "$SEED_2" --shielded-amount 5 --unshielded-amount 5 --destination-address "$ADDR_S3" --destination-address "$ADDR_U3"
t_check L9-TX-4 "HF-05" "one unshielded transfer to two destinations, seed 1 -> seeds 3 and 4" c_tx tx_multi_dest \
    --source-seed "$SEED_1" --unshielded-amount 5 --destination-address "$ADDR_U3" --destination-address "$ADDR_U4"

t_section "the saved ledger-8 transaction"
c_hf08() {
    [ -s "$STATE_DIR/v8_tx.xt" ] || { t_skip "no saved ledger-8 extrinsic (HF-PRE-1)"; return; }
    local xt v r now; xt=$(cat "$STATE_DIR/v8_tx.xt")
    v=$(validate_extrinsic "$xt"); r=$(submit_extrinsic "$xt"); printf 'validate %s\nsubmit %s\n' "$v" "$r" > "$EV/hf08_v8.txt"
    now=$(wallet_night "$EV/seed3_after_fork.json" 2>/dev/null)
    if ! night_kept "${SEED3_NIGHT:-}" "$now"; then t_fail "seed 3 NIGHT moved since HF-PRE-1 (${SEED3_NIGHT:-unrecorded} -> ${now:-unreadable}): its inputs may be spent"
    elif [ "$v" != 0x01000701 ]; then t_fail "validate_transaction $v, expected 0x01000701 (Custom error 1, DeserializationError); submit '$r'"
    elif [ "$r" = "1010 Invalid Transaction: Custom error: 1" ]; then t_pass "refused while decoding: $r; seed 3 still holds $now NIGHT (unshielded, shielded)"
    elif [[ "$r" == "1012 Transaction is temporarily banned"* ]]; then t_pass "banned by the pool after an earlier refusal, and validate_transaction still says Custom error 1; seed 3 still holds $now NIGHT (unshielded, shielded)"
    else t_fail "submit '$r', expected 1010 Custom error 1"; fi
}
c_hf08_corrupt() {
    local f="$EV/hf08_l9_tx.mn" xt v r code
    tk_chain generate-txs --dest-file "$f" single-tx --source-seed "$SEED_1" --unshielded-amount 1 \
        --destination-address "$ADDR_U4" > "$EV/hf08_l9_tx.log" 2>&1 && [ -s "$f" ] || { t_fail "$(last_line_of "$EV/hf08_l9_tx.log")"; return; }
    v=$(validate_extrinsic "$(bare_extrinsic "$f")")
    [[ "$v" == 0x00* ]] || { t_fail "the intact transaction is not valid either: $v"; return; }
    xt=$(bare_extrinsic "$f" --corrupt); r=$(submit_extrinsic "$xt"); printf 'intact %s\ncorrupt %s\n' "$v" "$r" > "$EV/hf08_corrupt.txt"
    code=$(grep -oE 'Custom error: [0-9]+$' <<< "$r" | grep -oE '[0-9]+$')
    case "$r" in
        accepted*) t_fail "the corrupted transaction was accepted: $r" ;;
        "1010 "*) [ -n "$code" ] && [ "$code" != 1 ] && t_pass "the intact copy validates; the corrupted one is refused by validation: $r" \
                      || t_fail "$r: expected a validation error, not the decoding error 1" ;;
        *) t_fail "${r:-no answer from the node}" ;;
    esac
}
c_hf08_after() {
    local n down=0 h1
    while read -r n; do [ -n "$n" ] || continue; [ -n "$(best_height_at "${n#*=}")" ] || down=$((down + 1)); done < <(printf '%s\n' "${NODES[@]}")
    [ "$down" = 0 ] || { t_fail "$down node(s) stopped answering"; return; }
    h1=$(node_height); sleep 13
    [ "$(node_height)" -gt "$h1" ] || { t_fail "block production stopped at $h1"; return; }
    tk_tx generate-txs single-tx --source-seed "$SEED_3" --unshielded-amount 2 --destination-address "$ADDR_U4" -d "$NODE_WS" \
        > "$EV/hf08_followup.log" 2>&1 && t_pass "nodes up, height $h1 -> $(node_height), a ledger-9 transfer from seed 3 landed" \
        || t_fail "follow-up transfer: $(last_line_of "$EV/hf08_followup.log")"
}
t_check L9-HF08-1 "HF-08" "the node refuses the saved ledger-8 transaction, submitted as a bare extrinsic; seed 3 never spent it" c_hf08
t_check L9-HF08-2 "HF-08" "the node refuses a ledger-9 transaction with one payload byte flipped (the intact copy validates)" c_hf08_corrupt
t_check L9-HF08-3 "HF-08" "the chain is unharmed: nodes up, blocks produced, a new transfer from seed 3 lands" c_hf08_after

c_new() {
    local rng f="$EV/deploy_c.mn"; rng=$(openssl rand -hex 32)
    tk_chain generate-txs --dest-file "$f" contract-simple deploy --funding-seed "$SEED_3" --rng-seed "$rng" > "$EV/cs_c.log" 2>&1 \
        && tk_send "$f" >> "$EV/cs_c.log" 2>&1 || { t_fail "deploy: $(last_line_of "$EV/cs_c.log")"; return; }
    CONTRACT_C=$(tk contract-address --src-file "$f" 2>/dev/null | tail -1)
    is_hex64 "$CONTRACT_C" || { t_fail "no address"; return; }
    st CONTRACT_C "$CONTRACT_C"; st RNG_C "$rng"; registry_add C contract-simple "$CONTRACT_C" l9
    local r; r=$(cs_call cs_c "$SEED_3" "$rng" "$CONTRACT_C" store check) && t_pass "C = $CONTRACT_C; store, check landed" || t_fail "$r"
}
t_check L9-NEW-1 "HF-06" "a contract deployed on ledger 9 (seed 3): deploy, store, check" c_new

# Ledger-8 artefacts cannot build a ledger-9 transaction through toolkit-js (node#1969).
# Recompiling and calling the pre-fork address with the pre-fork private state must work.
NODE_1969='Version mismatch: compiled code expects 0\.15\.[0-9]+, runtime is 0\.18|RuntimeApiError\(IncompatibleCodegen\)'

t_section "pre-fork Compact dApps"
COIN_PUBLIC_1=$(tk_address "$SEED_1" --coin-public)
c_dapp() {  # <name> <address> <circuit> [arg]
    local name=$1 addr=$2 circuit=$3 arg="${4:-}" keys orig=0 rc=0
    [ -n "$addr" ] || { t_skip "$name was not deployed before the fork"; return; }
    rm -f "$(dapp_dir "$name")/${DAPP_FILE_PREFIX}_${circuit}"_*.orig.log
    if dapp_call "$name" "$SEED_1" "$addr" "$circuit" ${arg:+"$arg"}; then orig=1; fi
    local f; for f in "$(dapp_dir "$name")/${DAPP_FILE_PREFIX}_${circuit}"_*.log; do [ -e "$f" ] && mv "$f" "${f%.log}.orig.log"; done
    if [ "$orig" = 0 ] && ! cat "$(dapp_dir "$name")/${DAPP_FILE_PREFIX}_${circuit}"_*.orig.log 2>/dev/null | grep -qE "$NODE_1969"; then
        t_fail "the original artefacts failed, but not with node#1969: $(tail -n 2 "$(dapp_dir "$name")/${DAPP_FILE_PREFIX}_${circuit}"_*.orig.log 2>/dev/null | tr '\n' ' ' | cut -c1-200)"; return
    fi
    keys=$(dapp_recompile "$name" "$TK_IMAGE" "$COIN_PUBLIC_1") || rc=$?
    [ "$rc" = 2 ] && { t_fail "recompile with compactc $L9_EXPECTED_COMPACTC failed: $keys"; return; }
    [ "$name" = bboard ] && [ "$orig" = 1 ] && { circuit=post; arg="posted-after-the-fork"; }
    DAPP_OUT=out-l9 dapp_call "$name" "$SEED_1" "$addr" "$circuit" ${arg:+"$arg"} \
        || { t_fail "$circuit with the recompiled artefacts: $(dapp_tail "$name" "${circuit}_send")"; return; }
    keys=$(echo "$keys" | tr '\n' ';')
    if [ "$orig" = 1 ]; then t_pass "original and recompiled artefacts both land; $keys"
    else t_known "original ledger-8 artefacts refused by toolkit-js with the node#1969 error (closed as intended); recompiled artefacts land on the pre-fork address; $keys"; fi
}
c_microdao() {
    [ -n "${DAPP_MICRO_DAO:-}" ] || { t_skip "micro-dao was not deployed before the fork (best effort)"; return; }
    local n; n=$(contract_semantic_state "$DAPP_MICRO_DAO" | grep -c '^op=' || true)
    [ "$n" -ge 5 ] && t_pass "$n entry points" || t_fail "$n entry points"
}
t_check L9-DAPP-1 "HF-02, HF-07" "counter.increment on the pre-fork address: original artefacts, then recompiled" c_dapp counter "${DAPP_COUNTER:-}" increment
t_check L9-DAPP-2 "HF-02, HF-07" "bboard.take_down with the pre-fork secret: original artefacts, then recompiled" c_dapp bboard "${DAPP_BBOARD:-}" take_down
t_check L9-DAPP-3 "HF-02" "micro-dao's state still exposes its entry points" c_microdao

c_rewards() {
    tk_tx generate-txs claim-rewards --funding-seed "$SEED_1" -d "$NODE_WS" > "$EV/claim_rewards.log" 2>&1 \
        && t_pass "claim-rewards landed" || { mv "$EV/claim_rewards.log" "$EV/claim_rewards.log.failed"
            t_warn "claim-rewards failed, possibly nothing to claim: $(last_line_of "$EV/claim_rewards.log.failed")"; }
}
t_check L9-REWARDS-1 "HF-04" "claim-rewards on ledger 9" c_rewards

t_section "ledger parameters"
fp() { grep -iE "$1" "$2" 2>/dev/null | sed -nE 's/.*FixedPoint\(([0-9.eE+-]+)\).*/\1/p' | head -1; }
c_params_static() {
    tk show-ledger-parameters -r "$NODE_WS" > "$EV/params.txt" 2>&1 && grep -q overall_price "$EV/params.txt" || { t_fail "$(last_line_of "$EV/params.txt")"; return; }
    [ -s "$STATE_DIR/params_l8.txt" ] || { t_skip "no ledger-8 record (L8-PARAM-1)"; return; }
    local gone new; gone=$(comm -23 <(param_lines "$STATE_DIR/params_l8.txt") <(param_lines "$EV/params.txt") | tr '\n' ';' | cut -c1-200)
    new=$(comm -13 <(param_lines "$STATE_DIR/params_l8.txt") <(param_lines "$EV/params.txt") | tr '\n' ';' | cut -c1-200)
    [ -z "$gone" ] && t_pass "static fields unchanged; ledger-9 additions: ${new:-none}" || t_warn "changed or gone: $gone (new: $new)"
}
c_params_floor() {
    local mbp op op8; mbp=$(fp min_block_price "$EV/params.txt"); op=$(fp overall_price "$EV/params.txt"); op8=$(fp overall_price "$STATE_DIR/params_l8.txt")
    [ -n "$mbp" ] || { t_fail "no min_block_price after the fork"; return; }
    python3 -c "import sys; sys.exit(0 if float('$op') >= float('$mbp') else 1)" || { t_fail "overall_price $op below min_block_price $mbp"; return; }
    t_pass "min_block_price $mbp; overall_price ${op8:-?} before the fork, $op after"
}
t_check L9-PARAM-1 "HF-09" "static ledger parameters unchanged; ledger-9 fields added" c_params_static
t_check L9-PARAM-2 "HF-09" "min_block_price is set and overall_price respects it" c_params_floor

c_fetch() {
    fetch_all "$EV/fetch.log" && t_pass "fetched to height $(node_height) across fork block #$FORK_HEIGHT" || t_fail "$(last_line_of "$EV/fetch.log")"
}
t_check L9-FETCH-1 "HF-16" "every block on both sides of the fork fetched and decoded by the ledger-9 toolkit" c_fetch

t_section "validators"
c_agree_sweep() {
    multi_node || { t_skip "one RPC endpoint (EXTRA_RPC_NODES adds more)"; return; }
    local from=$(( FORK_HEIGHT - 1 )) tip; tip=$(node_height)
    sweep_forks "$from" "$tip" > "$EV/fork_sweep.txt" 2>&1 && t_pass "hashes agree on $from..$tip" || t_fail "$(grep -m1 -E 'FORK|NO AGREEMENT' "$EV/fork_sweep.txt")"
}
c_agree_state() {
    multi_node || { t_skip "one RPC endpoint"; return; }
    local fin; fin=$(finalized_height)
    check_state_agreement "$(block_hash_at "$NODE_HTTP" "$fin")" > "$EV/state_agreement.txt" 2>&1 \
        && t_pass "stateRoot, ledger and zswap roots agree at finalized #$fin" || t_fail "$(head -3 "$EV/state_agreement.txt" | tr '\n' ' ')"
}
c_agree_spec() {
    local n bad="" cnt=0
    for n in "${NODES[@]}"; do
        cnt=$((cnt + 1)); [ "$(spec_version_at "${n#*=}")" = "$SPEC_NOW" ] || bad="$bad ${n%%=*}"
    done
    [ -z "$bad" ] && t_pass "$cnt/$cnt on spec $SPEC_NOW" || t_fail "unreachable or on another spec:$bad"
}
c_agree_finality() { local r; r=$(wait_finality_progress 2 90) && t_pass "$r" || t_fail "$r"; }
c_agree_cost() {
    logs_available || { t_skip "no node logs on this target"; return; }
    local n; : > "$EV/migration_cost.txt"
    for n in $(log_nodes); do
        node_logs "$n" | grep -oE 'translation complete in [0-9]+ step\(s\), [^,]+, [0-9]+ps synthetic cost' | tail -1 | sed "s/^/$n: /" >> "$EV/migration_cost.txt" || true
    done
    local distinct; distinct=$(grep -oE '[0-9]+ps synthetic cost' "$EV/migration_cost.txt" | sort -u | wc -l | tr -d ' ')
    [ "$distinct" = 1 ] && t_pass "$(head -1 "$EV/migration_cost.txt" | cut -d: -f2-) on $(wc -l < "$EV/migration_cost.txt" | tr -d ' ') nodes" \
        || t_fail "$distinct distinct values (evidence/l9/migration_cost.txt)"
}
t_check L9-AGREE-1 "HF-13" "every node reports the same block hashes from the fork block to the tip" c_agree_sweep
t_check L9-AGREE-2 "HF-13" "stateRoot, ledger and zswap roots agree at the finalized block" c_agree_state
t_check L9-AGREE-3 "HF-01" "every node runs the new runtime" c_agree_spec
t_check L9-AGREE-4 "HF-01" "finality advances" c_agree_finality
t_check L9-AGREE-5 "HF-15" "every validator reports the same migration cost for the fork block" c_agree_cost

t_section "the indexer across the fork"
c_idx_crossed() {
    if [ "$TARGET" = local ]; then
        docker inspect -f '{{.State.Running}}' chain-indexer 2>/dev/null | grep -q true || { t_fail "chain-indexer is not running"; return; }
        local m; m=$(docker logs chain-indexer 2>&1 | grep -aiE 'ledger state root mismatch|zswap state root mismatch|translate ledger state' | tail -1)
        [ -z "$m" ] || { t_fail "boundary error in chain-indexer: ${m:0:200}"; return; }
    fi
    local r; r=$(indexer_catch_up 180) && t_pass "indexer $(indexer_height) >= finalized, across fork block #$FORK_HEIGHT" || t_fail "$r"
}
c_idx_contracts() {
    local a missing="" n=0
    for a in $(registry_addresses); do n=$((n + 1)); [ -n "$(indexer_contract_type "$a")" ] || missing="$missing $(registry_label "$a")"; done
    [ -z "$missing" ] && t_pass "$n contracts queryable" || t_fail "missing:$missing"
}
c_idx_tip() {
    local b pv dge; b=$(indexer_block); pv=$(echo "$b" | jq -r '.protocolVersion // empty'); dge=$(echo "$b" | jq -r '.dustGenerationEndIndex // empty')
    [ "$pv" = "$SPEC_NOW" ] || { t_fail "protocolVersion $pv, spec $SPEC_NOW"; return; }
    if [ -n "$dge" ] && [ "$dge" -gt 0 ] && [ "$dge" -lt "${PRE_DUST_GEN_END:-0}" ]; then
        t_pass "protocolVersion ${PRE_PROTOCOL_VERSION:-?} -> $pv; dustGenerationEndIndex ${PRE_DUST_GEN_END:-?} -> $dge (reset, then re-registrations)"
    elif [ "$TARGET" = network ] && [ -n "$dge" ]; then
        t_warn "dustGenerationEndIndex ${PRE_DUST_GEN_END:-?} -> $dge: other wallets re-register on a shared network, so the drop may be hidden"
    else t_fail "dustGenerationEndIndex ${PRE_DUST_GEN_END:-?} -> ${dge:-null}, expected 0 < after < before"; fi
}
c_idx_inclusion() {
    local r; r=$(tx_inclusion_check "$EV/tx_inclusion.tsv" "$EV"/*.log "$DAPPS_DIR"/*/"${DAPP_FILE_PREFIX}"_*send.log) && t_pass "$r" || t_fail "$r"
}
t_check L9-IDX-1 "HF-12" "the indexer crossed the fork and follows the finalized tip" c_idx_crossed
t_check L9-IDX-2 "HF-12" "every registry contract is queryable (including contract C)" c_idx_contracts
t_check L9-IDX-3 "HF-12, HF-04" "the indexer tip: protocolVersion flipped, the DUST generation tree reset and refilled" c_idx_tip
t_check L9-IDX-4 "HF-12" "node and indexer serve the same state for every registry contract" c_node_vs_indexer "$FORK_HEIGHT"
t_check L9-IDX-5 "HF-12" "every transaction of this phase is in a block" c_idx_inclusion

t_finish
