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

# Table FEAT: what ledger 9 adds. FEAT_ONLY=EVT,ZKIR3 runs a subset against the state a
# full pass recorded.
set -e
source "$(dirname "${BASH_SOURCE[0]}")/../lib/suite.sh"

EV="$EVIDENCE_DIR/features"; mkdir -p "$EV"
use_toolkit l9
require_indexer
state_load "$PRE_FORK_STATE"
export DAPP_FILE_PREFIX
require_era l9
if [ -n "${FEAT_ONLY:-}" ]; then state_load "$FEATURES_STATE"; T_APPEND=1; else : > "$FEATURES_STATE"; fi
want() { [ -z "${FEAT_ONLY:-}" ] || [[ ",$(echo "$FEAT_ONLY" | tr a-z A-Z)," == *",$1,"* ]]; }
fst() { state_set "$FEATURES_STATE" "$1" "$2"; }
ECDSA_SEED="${ECDSA_SEED:-$SEED_4}"
ADDR_DEST=$(tk_address "$SEED_3" --unshielded)

t_table FEAT "What ledger 9 adds, on $NETWORK_NAME (spec $SPEC_NOW, toolkit $TK_IMAGE)"

if want ECDSA; then
t_section "ECDSA"
ecdsa_guard() { grep -q 'EcdsaNotSupportedForLedger(Ledger8)' "$1"; }
c_ecdsa_identity() {
    local ea sa
    ea=$(tk_address "ecdsa:$ECDSA_SEED" --unshielded); sa=$(tk_address "$ECDSA_SEED" --unshielded)
    [[ "$ea" == mn_addr_* ]] && [ "$ea" != "$sa" ] || { t_fail "no distinct ECDSA address ('$ea' vs '$sa')"; return; }
    tk_tx generate-txs single-tx --source-seed "$SEED_1" --unshielded-amount 1000 --destination-address "$ea" -d "$NODE_WS" \
        > "$EV/ecdsa_fund.log" 2>&1 || { t_fail "funding the ECDSA address: $(last_line_of "$EV/ecdsa_fund.log")"; return; }
    # The ECDSA identity holds no DUST; seed 1 pays the fee, the NIGHT input is ECDSA-signed.
    if tk_tx generate-txs single-tx --source-seed "ecdsa:$ECDSA_SEED" --funding-seed "$SEED_1" --unshielded-amount 1 \
            --destination-address "$ADDR_DEST" -d "$NODE_WS" > "$EV/ecdsa_spend.log" 2>&1; then
        fst ECDSA_OK 1; t_pass "ECDSA address $ea funded and spent with an ECDSA signature"
    elif ecdsa_guard "$EV/ecdsa_spend.log"; then
        fst ECDSA_OK 0; t_fail "the toolkit refuses ecdsa: seeds on a chain with ledger-8 history: node#2180 has regressed (fixed by node#2181)"
    else fst ECDSA_OK 0; t_fail "$(last_line_of "$EV/ecdsa_spend.log")"; fi
}
c_ecdsa_committee() {
    state_load "$FEATURES_STATE"
    [ "${ECDSA_OK:-0}" = 1 ] || { t_skip "no ECDSA spend (FEAT-ECDSA-1)"; return; }
    local rng f="$EV/deploy_ecdsa.mn" addr; rng=$(openssl rand -hex 32)
    tk_chain generate-txs --dest-file "$f" contract-simple deploy --funding-seed "$SEED_1" --rng-seed "$rng" \
        --authority-seed "ecdsa:$ECDSA_SEED" > "$EV/ecdsa_contract.log" 2>&1 && tk_send "$f" >> "$EV/ecdsa_contract.log" 2>&1 \
        || { t_fail "deploy: $(last_line_of "$EV/ecdsa_contract.log")"; return; }
    addr=$(tk contract-address --src-file "$f" 2>/dev/null | tail -1)
    fst CONTRACT_ECDSA "$addr"; fst RNG_ECDSA "$rng"; registry_add ECDSA contract-simple "$addr" l9
    local k
    for k in store check; do
        tk_tx generate-txs contract-simple call --funding-seed "$SEED_1" --call-key "$k" --rng-seed "$rng" --contract-address "$addr" \
            -d "$NODE_WS" >> "$EV/ecdsa_contract.log" 2>&1 || { t_fail "$k: $(last_line_of "$EV/ecdsa_contract.log")"; return; }
    done
    semantic_snapshot "$addr" "$EV/ecdsa_state.txt" && grep -qx 'authority_threshold=1' "$EV/ecdsa_state.txt" \
        && t_pass "$addr: 1-of-1 ECDSA committee; store, check landed" || t_fail "unexpected committee: $(grep authority "$EV/ecdsa_state.txt" | tr '\n' ' ')"
}
c_ecdsa_rotate() {
    state_load "$FEATURES_STATE"
    [ -n "${CONTRACT_ECDSA:-}" ] || { t_skip "no ECDSA contract (FEAT-ECDSA-2)"; return; }
    tk_tx generate-txs contract-simple maintenance --funding-seed "$SEED_1" --contract-address "$CONTRACT_ECDSA" \
        --authority-seed "ecdsa:$ECDSA_SEED" --new-authority-seed "ecdsa:$ECDSA_SEED" --new-authority-seed "$SEED_2" \
        --threshold 2 --counter 0 --rng-seed "$RNG_ECDSA" -d "$NODE_WS" > "$EV/ecdsa_rotate.log" 2>&1 \
        || { t_fail "$(last_line_of "$EV/ecdsa_rotate.log")"; return; }
    semantic_snapshot "$CONTRACT_ECDSA" "$EV/ecdsa_rotated.txt" || { t_fail "state unreadable"; return; }
    grep -qx 'authority_threshold=2' "$EV/ecdsa_rotated.txt" && grep -qx 'authority_counter=1' "$EV/ecdsa_rotated.txt" \
        || { t_fail "$(grep authority "$EV/ecdsa_rotated.txt" | tr '\n' ' ')"; return; }
    tk_tx generate-txs contract-simple call --funding-seed "$SEED_1" --call-key store --rng-seed "$RNG_ECDSA" \
        --contract-address "$CONTRACT_ECDSA" -d "$NODE_WS" >> "$EV/ecdsa_rotate.log" 2>&1 \
        && t_pass "ECDSA signature rotated the committee to ECDSA + Schnorr, threshold 2, counter 1; store still works" \
        || t_fail "store after the rotation: $(last_line_of "$EV/ecdsa_rotate.log")"
}
t_check FEAT-ECDSA-1 "HF-06" "an ECDSA NIGHT identity: distinct address, funded, spends with an ECDSA signature" c_ecdsa_identity
t_check FEAT-ECDSA-2 "HF-03, HF-06" "a contract with an ECDSA maintenance committee: deploy, store, check" c_ecdsa_committee
t_check FEAT-ECDSA-3 "HF-03" "an ECDSA-signed rotation to a mixed ECDSA + Schnorr 2-of-2 committee" c_ecdsa_rotate
fi

# toolkit-js takes byte arguments as bare hex
name32() { local h; h=$(printf '%s' "$1" | xxd -p -c 256); printf '%-64s' "$h" | tr ' ' '0'; }
EV_NAME=$(name32 'hard fork events')
EV_PAYLOAD=$(printf '%02x' {0..255})
EV_NULLIFIER=$(printf 'ee%.0s' {1..32})
hexify() {  # byte arrays in a JSON document as hex strings
    python3 - "$1" <<'PY'
import json, sys
def walk(v):
    if isinstance(v, list) and v and all(isinstance(x, int) and 0 <= x < 256 for x in v):
        return bytes(v).hex()
    if isinstance(v, dict):
        return {k: walk(x) for k, x in v.items()}
    if isinstance(v, list):
        return [walk(x) for x in v]
    return v
print(json.dumps(walk(json.load(open(sys.argv[1])))))
PY
}
if want EVT; then
t_section "contract events"
c_evt_emit() {
    local addr; addr=$(dapp_deploy events "$SEED_1" || true)
    is_hex64 "$addr" || { t_fail "deploy: $addr $(dapp_tail events deploy_send)"; return; }
    fst DAPP_EVENTS "$addr"; registry_add events compact "$addr" l9
    DAPP_EVENTS_OUT=1 dapp_call events "$SEED_1" "$addr" log "$EV_NAME" "$EV_PAYLOAD" || { t_fail "log(): $(dapp_tail events log_intent)"; return; }
    local f; f=$(dapp_file events_events.json); cp "$f" "$EV/events_log.json" 2>/dev/null || { t_fail "no --output-events file"; return; }
    [ "$(jq length "$f")" = 1 ] && hexify "$f" | grep -q "$EV_NAME" && hexify "$f" | grep -q "$EV_PAYLOAD" \
        || { t_fail "log() events: $(jq -c . "$f" | cut -c1-200)"; return; }
    DAPP_EVENTS_OUT=1 dapp_call events "$SEED_1" "$addr" mark "$EV_NULLIFIER" || { t_fail "mark(): $(dapp_tail events mark_intent)"; return; }
    cp "$f" "$EV/events_mark.json"
    hexify "$f" | grep -q "$EV_NULLIFIER" || { t_fail "mark() did not emit the nullifier"; return; }
    t_pass "events dApp $addr: log() emitted one Misc event (32-byte name, 256-byte payload), mark() one ShieldedSpend"
}
c_evt_indexer() {
    state_load "$FEATURES_STATE"
    [ -n "${DAPP_EVENTS:-}" ] || { t_skip "no events dApp (FEAT-EVT-1)"; return; }
    local r; r=$(indexer_catch_up 180) || { t_fail "$r"; return; }
    local q res n name payload null nss npfx nca
    is() { [ "${1#0x}" = "$2" ] && echo y || echo n; }
    q="{ contractEvents(filter: { contractAddress: \"$DAPP_EVENTS\" }) { __typename protocolVersion contractAddress ... on MiscContractEvent { name payload } ... on ShieldedSpendEvent { nullifier } } }"
    res=$(gql_query "$q"); echo "$res" > "$EV/events_indexer.json"
    echo "$res" | jq -e '.errors' >/dev/null 2>&1 && { t_fail "query rejected: $(echo "$res" | jq -c .errors | cut -c1-200)"; return; }
    n=$(echo "$res" | jq '.data.contractEvents | length')
    name=$(echo "$res" | jq -r '.data.contractEvents[] | select(.__typename == "MiscContractEvent") | .name' | head -1)
    payload=$(echo "$res" | jq -r '.data.contractEvents[] | select(.__typename == "MiscContractEvent") | .payload' | head -1)
    null=$(echo "$res" | jq -r '.data.contractEvents[] | select(.__typename == "ShieldedSpendEvent") | .nullifier' | head -1)
    [ "$n" = 2 ] && [ "${name#0x}" = "$EV_NAME" ] && [ "${payload#0x}" = "$EV_PAYLOAD" ] && [ "${null#0x}" = "$EV_NULLIFIER" ] \
        || { t_fail "$n events; name/payload/nullifier match: $(is "$name" "$EV_NAME")/$(is "$payload" "$EV_PAYLOAD")/$(is "$null" "$EV_NULLIFIER")"; return; }
    nss=$(gql_query "{ contractEvents(filter: { contractAddress: \"$DAPP_EVENTS\", types: [SHIELDED_SPEND] }) { __typename } }" | jq '.data.contractEvents | length')
    npfx=$(gql_query "{ contractEvents(filter: { contractAddress: \"$DAPP_EVENTS\", types: [SHIELDED_SPEND], fieldPrefixes: [{ fieldName: \"nullifier\", prefix: \"${EV_NULLIFIER:0:8}\" }] }) { __typename } }" | jq '.data.contractEvents | length' 2>/dev/null)
    nca=$(gql_query "{ contractAction(address: \"$DAPP_EVENTS\") { ... on ContractCall { contractEvents { __typename } } } }" | jq '.data.contractAction.contractEvents | length' 2>/dev/null)
    [ "$nss" = 1 ] || { t_fail "type filter returned $nss events, expected 1"; return; }
    [ "$nca" = 1 ] || { t_fail "the latest ContractCall carries $nca events, expected 1"; return; }
    [ "$npfx" = 1 ] && t_pass "2 events with the emitted bytes; type filter, nullifier-prefix filter and per-call attribution exact" \
        || t_warn "2 events, type filter and attribution exact; the fieldPrefixes filter returned '$npfx'"
}
t_check FEAT-EVT-1 "HF-10" "a dApp emits a Misc and a ShieldedSpend event; the toolkit reports both" c_evt_emit
t_check FEAT-EVT-2 "HF-10, HF-12" "the indexer serves both events with the emitted bytes; filters and per-call attribution" c_evt_indexer
fi

if want ZKIR3; then
t_section "ZKIR v3"
c_zkir3() {
    local addr pre expect prover=embedded
    addr=$(dapp_deploy keccak "$SEED_1" || true)
    is_hex64 "$addr" || { t_fail "deploy: $addr $(dapp_tail keccak deploy_send)"; return; }
    fst DAPP_KECCAK "$addr"; registry_add keccak compact "$addr" l9
    pre=$(printf 'a1%.0s' {1..32}); expect=$(python3 "$SUITE_DIR/dapps/keccak256.py" "$pre")
    if ! DAPP_RESULT_OUT=1 dapp_call keccak "$SEED_1" "$addr" hash "$pre"; then
        grep -q 'ir-source\[v3' "$(dapp_log keccak hash_prove)" 2>/dev/null || { t_fail "hash(): $(dapp_tail keccak hash_prove)"; return; }
        cp "$(dapp_log keccak hash_prove)" "$EV/zkir3_embedded_prover.log"
        proof_server_up l9 >/dev/null 2>&1 || { t_fail "the embedded prover refused ZKIR v3 and no ledger-9 proof server answers"; return; }
        prover="proof server $PS_L9_TAG"
        PROOF_SERVER="$PS_L9_URL" DAPP_RESULT_OUT=1 dapp_call keccak "$SEED_1" "$addr" hash "$pre" \
            || { t_fail "hash() at the proof server: $(dapp_tail keccak hash_send)"; return; }
    fi
    fst ZKIR3_PROVER "$prover"
    local r; r=$(dapp_file keccak_result.json)
    [ -s "$r" ] && hexify "$r" | grep -q "$expect" && t_pass "V3 proof from the $prover verified on chain; keccak256 = $expect (dapps/keccak256.py)" \
        || t_fail "the circuit result does not contain $expect: $(cut -c1-160 "$r" 2>/dev/null)"
}
c_zkir3_embedded() {
    state_load "$FEATURES_STATE"
    case "${ZKIR3_PROVER:-}" in
        embedded) t_pass "the toolkit's embedded prover proved the V3 circuit" ;;
        "") t_skip "FEAT-ZKIR3-1 did not reach a proof" ;;
        *) t_warn "not verified: the toolkit's embedded prover accepts ZKIR v2 only (a toolkit limitation, not filed) ($(grep -m1 -o "expected one of[^\"]*" "$EV/zkir3_embedded_prover.log" | cut -c1-120)); the proof server proves V3 and the chain verifies it" ;;
    esac
}
t_check FEAT-ZKIR3-1 "HF-06" "a keccak256 circuit compiled for ZKIR v3 deploys, proves and verifies; the digest matches" c_zkir3
t_check FEAT-ZKIR3-2 "HF-06" "the toolkit's embedded prover proves a ZKIR v3 circuit" c_zkir3_embedded
fi

if want CCC; then
t_section "cross-contract calls"
c_ccc_deploy() {
    state_load "$FEATURES_STATE"
    [ -n "${DAPP_EVENTS:-}" ] || { t_skip "needs the events dApp (FEAT-EVT-1)"; return; }
    local addr style=json
    addr=$(CCC_CALLEE_ADDRESS="$DAPP_EVENTS" dapp_deploy ccc-outer "$SEED_1" || true)
    if ! is_hex64 "$addr"; then
        style=hex; addr=$(CCC_CTOR_STYLE=hex CCC_CALLEE_ADDRESS="$DAPP_EVENTS" dapp_deploy ccc-outer "$SEED_1" || true)
    fi
    is_hex64 "$addr" || { t_fail "deploy: $(dapp_tail ccc-outer deploy_intent)"; return; }
    fst DAPP_CCC_OUTER "$addr"; registry_add ccc-outer compact "$addr" l9
    t_pass "ccc-outer $addr with the events contract as callee (constructor argument as $style)"
}
c_ccc_call() {
    state_load "$FEATURES_STATE"
    [ -n "${DAPP_CCC_OUTER:-}" ] || { t_skip "no outer contract (FEAT-CCC-1)"; return; }
    if DAPP_EVENTS_OUT=1 dapp_call ccc-outer "$SEED_1" "$DAPP_CCC_OUTER" relay "$(name32 relayed)" "$EV_PAYLOAD"; then
        local r n_tx n_callee n_outer; r=$(indexer_catch_up 180) || { t_fail "$r"; return; }
        n_tx=$(jq length "$(dapp_file ccc-outer_events.json)")
        n_callee=$(gql_query "{ contractEvents(filter: { contractAddress: \"$DAPP_EVENTS\" }) { __typename } }" | jq '.data.contractEvents | length')
        n_outer=$(gql_query "{ contractEvents(filter: { contractAddress: \"$DAPP_CCC_OUTER\" }) { __typename } }" | jq '.data.contractEvents | length')
        [ "$n_tx" = 2 ] && [ "$n_callee" = 3 ] && [ "$n_outer" = 1 ] && t_pass "one transaction, events from two addresses, attributed per address" \
            || t_fail "events in the tx $n_tx (2), callee $n_callee (3), outer $n_outer (1)"
    elif grep -q 'Expected state provider for call to' "$(dapp_log ccc-outer relay_intent)" 2>/dev/null; then
        t_warn "not verified: toolkit-js cannot execute a cross-contract call, it takes one on-chain state and has no flag for the callee's (not filed)"
    else t_fail "relay(): $(dapp_tail ccc-outer relay_intent)"; fi
}
t_check FEAT-CCC-1 "HF-06" "an outer contract with the events contract as its callee deploys" c_ccc_deploy
t_check FEAT-CCC-2 "HF-06, HF-10" "relay(): the outer contract calls the events contract; two events from two addresses in one tx" c_ccc_call
fi

if want BRIDGE; then
t_section "cNIGHT -> NIGHT bridge"
clients_ready > /dev/null 2>&1 || true
c_bridge_pallet() {
    local p cand mcs
    for cand in bridge c2mBridge; do
        p=$(chain_check storage --pallet "$cand" 2>"$EV/bridge_pallet.err") && echo "$p" | jq -e '.present == true' >/dev/null 2>&1 && break
        p=""
    done
    [ -n "$p" ] || { t_fail "no bridge pallet readable: $(last_line_of "$EV/bridge_pallet.err")"; return; }
    mcs=$(chain_check storage --pallet "$cand" --item "${BRIDGE_MCS_ITEM:-mainChainScriptsConfiguration}" 2>/dev/null | jq -c '.value // null')
    if [ -n "$mcs" ] && [ "$mcs" != null ]; then t_pass "pallet '$cand' present; main-chain scripts configured: $(echo "$mcs" | cut -c1-160)"
    else t_warn "pallet '$cand' present, but no main-chain scripts are set: the bridge stays inert until governance sets them"; fi
}
c_bridge_transfer() {
    [ "$TARGET" = network ] && [ "${BRIDGE_TEST:-0}" = 1 ] || { t_skip "a live transfer needs a network with a Cardano side and BRIDGE_TEST=1"; return; }
    local missing="" v n0 n1 waited=0
    for v in BRIDGE_SIGNING_KEY BRIDGE_ICS_CONFIG OGMIOS_URL BRIDGE_RECIPIENT_HEX; do [ -n "${!v:-}" ] || missing="$missing $v"; done
    [ -z "$missing" ] || { t_fail "BRIDGE_TEST=1 but missing:$missing"; return; }
    wallet_snapshot "$SEED_4" "$EV/bridge_before.json" || true; n0=$(wallet_night_unshielded "$EV/bridge_before.json" 2>/dev/null || echo 0)
    tk bridge-transfer --signing-key "$BRIDGE_SIGNING_KEY" --ics-config "$BRIDGE_ICS_CONFIG" --ogmios-url "$OGMIOS_URL" \
        --recipient-address "$BRIDGE_RECIPIENT_HEX" --amount "${BRIDGE_AMOUNT:-5}" > "$EV/bridge_transfer.log" 2>&1 \
        || { t_fail "$(last_line_of "$EV/bridge_transfer.log")"; return; }
    while [ "$waited" -lt "${BRIDGE_WAIT_S:-1800}" ]; do
        sleep 120; waited=$((waited + 120))
        wallet_snapshot "$SEED_4" "$EV/bridge_after.json" || continue
        n1=$(wallet_night_unshielded "$EV/bridge_after.json"); big_gt "$n1" "$n0" && { t_pass "seed 4 NIGHT $n0 -> $n1"; return; }
    done
    t_fail "no NIGHT credited to seed 4 within ${BRIDGE_WAIT_S:-1800} s (still $n0)"
}
t_check FEAT-BRIDGE-1 "HF-04" "the bridge pallet is in the runtime and its main-chain scripts are configured" c_bridge_pallet
t_check FEAT-BRIDGE-2 "HF-04" "a cNIGHT -> NIGHT user transfer is credited (network, BRIDGE_TEST=1)" c_bridge_transfer
fi

if want SCOPE; then
t_section "ledger-9 scope not exercised here"
c_rsa() { t_skip "not covered: no contract with an RSA-1024/2048 verification circuit and signed vector yet"; }
c_usdcx() { t_skip "not covered beyond native keccak256 (FEAT-ZKIR3-1): the USDCx hashing vectors are not wired into a contract"; }
t_check FEAT-RSA-1 "-" "RSA signature verification inside a circuit" c_rsa
t_check FEAT-USDCX-1 "-" "USDCx hashing requirements end to end" c_usdcx
fi

t_finish
