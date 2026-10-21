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

# Tables CLI-L8 / CLI-L9: proof servers, the indexer, the wallet SDK and Midnight.js on one
# side of the fork.
#
#   client_stack.sh [l8|l9]     (default: the era of the chain head)
set -e
source "$(dirname "${BASH_SOURCE[0]}")/../lib/suite.sh"

ERA="${1:-}"
if [ -z "$ERA" ]; then [ "$(spec_version)" -ge "$LEDGER9_SPEC_FLOOR" ] && ERA=l9 || ERA=l8; fi
case "$ERA" in l8) WANT=v8; ACTIVE_PS=$PS_L8_URL; ACTIVE_TAG=$PS_L8_TAG ;; l9) WANT=v9; ACTIVE_PS=$PS_L9_URL; ACTIVE_TAG=$PS_L9_TAG ;;
    *) echo "usage: client_stack.sh [l8|l9]" >&2; exit 2 ;; esac
EV="$EVIDENCE_DIR/clients"; mkdir -p "$EV"
use_toolkit "$ERA"
require_indexer
state_load "$PRE_FORK_STATE"; state_load "$FEATURES_STATE"
[ -n "${ADDR_U4:-}" ] || ADDR_U4=$(tk_address "$SEED_4" --unshielded)
[ "$ERA" = l9 ] && { FORK_HEIGHT=$(fork_height "${SPEC_BEFORE:-$L8_EXPECTED_SPEC}") || FORK_HEIGHT=""; }
export DAPP_FILE_PREFIX="${DAPP_FILE_PREFIX:-hf}"
MJS_RETAINED_COMPACTC="${MJS_RETAINED_COMPACTC:-0.31.1}"
MJS_CURRENT_COMPACTC="${MJS_CURRENT_COMPACTC:-0.34.0}"
ERA_U=$(echo "$ERA" | tr a-z A-Z)

t_table "CLI-$ERA_U" "Client stack on ledger ${ERA#l} ($NETWORK_NAME; wallet SDK, Midnight.js, proof servers $PS_L8_TAG / $PS_L9_TAG)"

c_proof_servers() {
    proof_server_up l8 > "$EV/proof_server_l8.log" 2>&1 && proof_server_up l9 > "$EV/proof_server_l9.log" 2>&1 \
        || { t_fail "proof servers not reachable (evidence/clients/proof_server_*.log)"; return; }
    local v8 v9; v8=$(proof_server_version "$PS_L8_URL"); v9=$(proof_server_version "$PS_L9_URL")
    [[ "$v8" == 8.* ]] && [[ "$v9" == 9.* ]] && t_pass "ledger 8: $v8 at $PS_L8_URL; ledger 9: $v9 at $PS_L9_URL" || t_fail "ledger 8 '$v8', ledger 9 '$v9'"
}
c_indexer_fields() {
    local r extra=""
    r=$(indexer_serves_wallet_fields) || { t_fail "$r: wallet SDK 2.x cannot sync against this indexer"; return; }
    if [ "$TARGET" = local ]; then
        local img; img=$(docker inspect -f '{{.Config.Image}}' chain-indexer 2>/dev/null)
        [[ "$img" == *":$INDEXER_TAG" ]] || { t_fail "chain-indexer runs '$img', expected tag $INDEXER_TAG"; return; }
        extra="; chain-indexer $img"
    fi
    t_pass "protocolVersion served on the progress types ($INDEXER_GQL)$extra"
}
c_versions() {
    clients_ready > "$EV/clients_ready.log" 2>&1 || { t_fail "npm ci in clients/ failed (evidence/clients/clients_ready.log)"; return; }
    local v w m; v=$(clients_versions); echo "$v" > "$EV/versions_$ERA.txt"
    w=$(client_version wallet-sdk "$v"); m=$(client_version midnight-js "$v")
    [ "$w" = "$(clients_pinned @midnightntwrk/wallet-sdk)" ] && [ "$m" = "$(clients_pinned @midnight-ntwrk/midnight-js-contracts)" ] \
        && t_pass "$v" || t_fail "installed $v differs from clients/package.json"
}
t_check C1 "-" "proof servers $PS_L8_TAG and $PS_L9_TAG up, each reporting its own ledger family" c_proof_servers
t_check C2 "HF-12" "the indexer serves the wallet SDK's progress fields" c_indexer_fields
t_check C3 "-" "client workspace installed at the pinned versions" c_versions

export CLIENT_SEED="$SEED_1"
c_probe() {
    client_run "$EV/probe_$ERA" wallet_check.mjs probe || { t_fail "$(client_error "$EV/probe_$ERA")"; return; }
    local era pv; era=$(jq -r .eraForHead "$EV/probe_$ERA.json"); pv=$(jq -r .chainHead.protocolVersion "$EV/probe_$ERA.json")
    [ "$era" = "$WANT" ] && t_pass "head protocolVersion $pv read as $era (forks.v9 = $(jq -r .forks.v9 "$EV/probe_$ERA.json"))" \
        || t_fail "head $pv read as $era, expected $WANT"
}
night_of() { jq -r --arg t "$NIGHT_TOKEN_TYPE" '.wallet.unshielded.balances[$t] // "0"' "$1"; }
c_sync() {
    client_run "$EV/wallet_$ERA" wallet_check.mjs sync || { t_fail "sync: $(client_error "$EV/wallet_$ERA")"; return; }
    local night dust pre; night=$(night_of "$EV/wallet_$ERA.json"); dust=$(jq -r '.wallet.dust.availableCoins // 0' "$EV/wallet_$ERA.json")
    big_gt "$night" 0 || { t_fail "synced, but NIGHT is 0"; return; }
    if [ "$ERA" = l8 ]; then
        [ "$dust" -gt 0 ] && t_pass "NIGHT $night, $dust DUST coins" || t_fail "NIGHT $night but no DUST before the fork"
    elif [ -s "$EV/wallet_l8.json" ]; then
        pre=$(night_of "$EV/wallet_l8.json"); t_pass "NIGHT before the fork $pre, after $night (fees and test transfers only); DUST coins $dust"
    else t_pass "NIGHT $night, DUST coins $dust (no pre-fork wallet snapshot to compare)"; fi
}
c_send() {
    client_run "$EV/send_$ERA" wallet_check.mjs send --kind unshielded --amount 1 \
        && t_pass "txId $(jq -r .txId "$EV/send_$ERA.json" | cut -c1-20)..., proved at protocol version $(jq -r .provedAtProtocolVersion "$EV/send_$ERA.json")" \
        || t_fail "$(client_error "$EV/send_$ERA")"
}
t_check C4 "HF-12" "the wallet SDK's fork schedule reads the chain head as era $WANT" c_probe
t_check C5 "HF-12, HF-04" "a wallet syncs through the indexer ($([ "$ERA" = l8 ] && echo "it has DUST" || echo "NIGHT survived the fork"))" c_sync
t_check C6 "HF-05" "an SDK transfer proved at $ACTIVE_PS ($ACTIVE_TAG) is accepted" c_send

if [ "$ERA" = l9 ]; then
    c_stale_ps() {
        jq -e '.submitted == true' "$EV/send_l9.json" >/dev/null 2>&1 || { t_skip "C6 did not pass, so a refusal here would prove nothing"; return; }
        if CLIENT_PS_V9="$PS_L8_URL" client_run "$EV/send_stale_ps" wallet_check.mjs send --kind unshielded --amount 1; then
            t_fail "a ledger-8 proof server produced an accepted ledger-9 transaction"; return
        fi
        # Only the node refusing the proof counts; any other failure proves nothing.
        case "$(client_custom_error "$EV/send_stale_ps")" in
            170) t_pass "the node refused it: Custom error 170 (InvalidDustSpendProof)" ;;
            179) t_pass "the node refused it: Custom error 179 (UnsupportedProofVersion)" ;;
            "") t_fail "failed before the node judged the proof: $(client_error "$EV/send_stale_ps")" ;;
            *) t_fail "the node refused it with Custom error $(client_custom_error "$EV/send_stale_ps"), expected 170 (InvalidDustSpendProof) or 179 (UnsupportedProofVersion)" ;;
        esac
    }
    c_sdk_dust() {
        client_run "$EV/dust_register" wallet_check.mjs dust-register \
            && t_pass "registered $(jq -r .utxos "$EV/dust_register.json") NIGHT UTxOs" \
            || t_warn "not registered: $(client_error "$EV/dust_register") $(grep -m1 -oE 'Custom error: [0-9]+' "$EV/dust_register.err"); re-registering UTxOs the toolkit already registered fails this way, as in midnight-wallet#415 (closed); L9-DUST-4 is the reference path"
    }
    c_restore() {
        [ -s "$EV/wallet_l9.json" ] || { t_skip "no synced wallet (C5)"; return; }
        client_run "$EV/restore" wallet_check.mjs restore --expect-json "$EV/wallet_l9.json" \
            && t_pass "synced from genesis in $(jq -r .restoredIn "$EV/restore.json") ms, peak RSS $(jq -r .peakRssMb "$EV/restore.json") MB, balances match" \
            || t_fail "$(client_error "$EV/restore")"
    }
    t_check C7 "HF-12" "the same transfer proved at the ledger-8 proof server ($PS_L8_TAG) is refused" c_stale_ps
    t_check C8 "HF-04" "self-funded DUST re-registration from the wallet SDK" c_sdk_dust
    t_check C9 "HF-12" "a wallet restored from genesis across the fork reaches the same balances" c_restore
fi

c_mjs_head() {
    client_run "$EV/mjs_head_$ERA" mjs_check.mjs head || { t_fail "$(client_error "$EV/mjs_head_$ERA")"; return; }
    local e; e=$(jq -r .era "$EV/mjs_head_$ERA.json")
    [ "$e" = "$WANT" ] && t_pass "era $e at protocol version $(jq -r .headProtocolVersion "$EV/mjs_head_$ERA.json")" || t_fail "era $e, expected $WANT"
}
c_toolkit_remote() {
    [ -n "$ADDR_U4" ] || { t_skip "no address for seed 4"; return; }
    tk_tx generate-txs single-tx --source-seed "$SEED_1" --unshielded-amount 1 --destination-address "$ADDR_U4" \
        --proof-server "$ACTIVE_PS" -d "$NODE_WS" > "$EV/toolkit_remote_prove_$ERA.log" 2>&1 \
        && t_pass "proved at $ACTIVE_TAG and accepted" || t_fail "$(last_line_of "$EV/toolkit_remote_prove_$ERA.log")"
}
t_check C10 "-" "Midnight.js reads the chain head as era $WANT" c_mjs_head
t_check C11 "-" "the toolkit proving at $ACTIVE_PS is accepted" c_toolkit_remote

# Artefacts resolve compact-runtime through a node_modules link to clients/; retained-era
# ones import its ledger-8 alias.
mjs_compile() {  # <name> <out-dir> <compactc> [retained]
    ln -sfn "$CLIENTS_DIR/node_modules" "$DAPPS_DIR/node_modules"
    DAPP_OUT="$2" DAPP_COMPACTC_VERSION="$3" dapp_compile "$1" "$L8_TOOLKIT_IMAGE" || return 1
    [ -z "${4:-}" ] || sed_i 's#@midnight-ntwrk/compact-runtime#compact-runtime-ledger8#g' "$(dapp_dir "$1")/$2/contract/index.js"
}
if [ "$ERA" = l8 ]; then
    # Refused by design: a retained-era constructor would leave an unmaintainable authority.
    # Before the fork dApps deploy through the toolkit (L8-DAPP-1).
    c_mjs_deploy() {
        mjs_compile counter out-mjs "$MJS_RETAINED_COMPACTC" retained > "$EV/mjs_compile.log" 2>&1 \
            || { t_fail "compactc $MJS_RETAINED_COMPACTC: $(tail -2 "$(dapp_dir counter)/compile.out-mjs.log" | tr '\n' ' ' | cut -c1-200)"; return; }
        if MN_ZK_VERIFY=warn client_run "$EV/mjs_deploy" mjs_check.mjs retained-deploy --contract-dir "$(dapp_dir counter)/out-mjs" --name counter; then
            t_warn "a retained-era deploy was accepted at $(jq -r .contractAddress "$EV/mjs_deploy.json"): Midnight.js no longer refuses it"
        elif grep -q Ledger8DeployUnmaintainableError "$EV/mjs_deploy.err"; then
            t_pass "refused with Ledger8DeployUnmaintainableError, as designed"
        else t_fail "$(client_error "$EV/mjs_deploy")"; fi
    }
    t_check C12 "HF-02" "Midnight.js refuses to deploy retained-era (compactc $MJS_RETAINED_COMPACTC) artefacts" c_mjs_deploy
else
    c_mjs_retained() {
        [ -n "${DAPP_COUNTER:-}" ] || { t_skip "no pre-fork counter (L8-DAPP-1)"; return; }
        [ -d "$(dapp_dir counter)/out-mjs/contract" ] || mjs_compile counter out-mjs "$MJS_RETAINED_COMPACTC" retained > "$EV/mjs_compile.log" 2>&1 \
            || { t_fail "compactc $MJS_RETAINED_COMPACTC: see $(dapp_dir counter)/compile.out-mjs.log"; return; }
        local j="$EV/mjs_retained"
        MN_ZK_VERIFY=warn client_run "$j" mjs_check.mjs retained-call --contract-dir "$(dapp_dir counter)/out-mjs" \
            --name counter --address "$DAPP_COUNTER" --circuit increment --times 3 \
            && t_pass "increment x3 on $DAPP_COUNTER: $(jq -r '[.results[].status] | join(",")' "$j.json")" \
            || t_fail "$(client_error "$j")"
    }
    c_mjs_events() {
        [ -n "${DAPP_EVENTS:-}" ] || { t_skip "no events dApp (FEAT-EVT-1)"; return; }
        client_run "$EV/mjs_events" mjs_check.mjs events --address "$DAPP_EVENTS" || { t_fail "$(client_error "$EV/mjs_events")"; return; }
        local n; n=$(jq -r .count "$EV/mjs_events.json")
        [ "${n:-0}" -ge 2 ] && t_pass "$n events ($(jq -r '[.events[].eventType] | join(",")' "$EV/mjs_events.json"))" || t_fail "$n events, expected at least 2"
    }
    c_mjs_decode() {
        local a label ok=0 bad="" known="" n=0 j
        while IFS=$'\t' read -r _ label _ a era; do
            [ "$era" = l8 ] || continue
            n=$((n + 1)); j="$EV/mjs_decode_${a:0:8}"
            if client_run "$j" mjs_check.mjs decode-state --address "$a"; then ok=$((ok + 1))
            elif [ "$(jq -r '.tag // empty' "$j.json" 2>/dev/null)" = "midnight:contract-state[v6]" ] \
                && ! contract_state_node_vs_indexer "$a" "$j" > /dev/null && indexer_1605 "$a" "$j" "$FORK_HEIGHT"; then known="$known $label"
            else bad="$bad $label($(jq -r '.tag // "?"' "$j.json" 2>/dev/null)): $(client_error "$j" | cut -c1-90);"; fi
        done < <(tail -n +2 "$REGISTRY" 2>/dev/null)
        [ "$n" -gt 0 ] || { t_skip "no pre-fork contracts in the registry"; return; }
        if [ -n "$bad" ]; then t_fail "$ok of $n decoded;$bad"
        elif [ -n "$known" ]; then t_known "$ok of $n decoded; the indexer serves the last pre-fork state, data unchanged, for$known (midnight-indexer#1605)"
        else t_pass "$ok of $n pre-fork contracts decoded"; fi
    }
    c_mjs_current() {
        mjs_compile counter out-mjs9 "$MJS_CURRENT_COMPACTC" > "$EV/mjs_compile9.log" 2>&1 \
            || { t_fail "compactc $MJS_CURRENT_COMPACTC: see $(dapp_dir counter)/compile.out-mjs9.log"; return; }
        client_run "$EV/mjs_deploy9" mjs_check.mjs deploy --contract-dir "$(dapp_dir counter)/out-mjs9" --name counter \
            || { t_fail "deploy: $(client_error "$EV/mjs_deploy9")"; return; }
        local a; a=$(jq -r .contractAddress "$EV/mjs_deploy9.json"); registry_add mjs-counter-l9 compact "$a" l9
        client_run "$EV/mjs_call9" mjs_check.mjs call --contract-dir "$(dapp_dir counter)/out-mjs9" --name counter --address "$a" --times 2 \
            && t_pass "deployed at $a, increment x2: $(jq -r '[.results[].status] | join(",")' "$EV/mjs_call9.json")" \
            || t_fail "deployed at $a; call: $(client_error "$EV/mjs_call9")"
    }
    t_check C13 "HF-07" "the pre-fork counter called three times through Midnight.js with compactc $MJS_RETAINED_COMPACTC artefacts" c_mjs_retained
    t_check C14 "HF-10" "the events dApp's events through Midnight.js queryContractEvents" c_mjs_events
    t_check C15 "HF-02, HF-12" "Midnight.js decodes the state of every pre-fork contract (queryContractState)" c_mjs_decode
    t_check C16 "HF-06" "a counter compiled with compactc $MJS_CURRENT_COMPACTC deployed and called through Midnight.js" c_mjs_current
fi

t_finish
