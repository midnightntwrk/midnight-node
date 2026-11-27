#!/usr/bin/env bash
# This file is part of midnight-node.
# Copyright (C) Midnight Foundation
# SPDX-License-Identifier: Apache-2.0
# Licensed under the Apache License, Version 2.0 (the "License");
# You may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#	http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

# Group 7, what ledger 9 adds: ECDSA signatures, contract events, ZKIR v3, cross-contract
# calls, the cNIGHT -> NIGHT bridge and a minimum block price. One end-to-end check for each,
# through the indexer and Midnight.js where they expose it, and a row for each feature
# nothing covers yet.
#
# Sourced by groups/g07_new_ledger_features.sh: the wallets are the fresh ones, and a
# contract deployed here is recorded as fresh-<name>.

# toolkit-js takes byte arguments as bare hex.
as_32_bytes() {  # <text> -> its bytes, zero-padded to 32, as hex
    local hex; hex=$(printf '%s' "$1" | xxd -p -c 256)
    printf '%-64s' "$hex" | tr ' ' '0'
}

# The toolkit writes byte arrays as lists of numbers; this prints the document with them as hex.
with_bytes_as_hex() {  # <json file>
    python3 - "$1" <<'PY'
import json, sys

def convert(value):
    if isinstance(value, list) and value and all(isinstance(x, int) and 0 <= x < 256 for x in value):
        return bytes(value).hex()
    if isinstance(value, dict):
        return {key: convert(x) for key, x in value.items()}
    if isinstance(value, list):
        return [convert(x) for x in value]
    return value

print(json.dumps(convert(json.load(open(sys.argv[1])))))
PY
}

# --- ECDSA signatures -------------------------------------------------------------------------
# A second signature scheme for NIGHT and for contract maintenance. The ECDSA identity is the
# spare wallet's seed under the ecdsa scheme; it holds no DUST, so the sender pays the fees.
ECDSA_IDENTITY="ecdsa:$SPARE_SEED"

check_ecdsa_wallet_spends() {
    local ecdsa_address problem
    ecdsa_address=$(tk_address "$ECDSA_IDENTITY" --unshielded)
    [[ "$ecdsa_address" == mn_addr_* ]] && [ "$ecdsa_address" != "$SPARE_ADDRESS" ] \
        || { fail "the ECDSA identity has no address of its own ('$ecdsa_address', Schnorr '$SPARE_ADDRESS')"; return; }
    problem=$(send_night ecdsa_funding --source-seed "$SENDER_SEED" --unshielded-amount 1000 --destination-address "$ecdsa_address") \
        || { fail "funding the ECDSA address: $problem"; return; }
    if problem=$(send_night ecdsa_spend --source-seed "$ECDSA_IDENTITY" --funding-seed "$SENDER_SEED" --unshielded-amount 1 \
            --destination-address "$RECEIVER_ADDRESS"); then
        pass "the ECDSA address $ecdsa_address was funded and spent from, with an ECDSA signature"
    elif grep -q 'EcdsaNotSupportedForLedger(Ledger8)' "$EV/ecdsa_spend.log"; then
        fail "the toolkit refuses ECDSA seeds on a chain with ledger-8 history: node#2180 has regressed (fixed by node#2181)"
    else
        fail "$problem"
    fi
}

check_ecdsa_contract_committee() {
    local address problem
    address=$(store_contract_deploy fresh-ecdsa-store "$SENDER_SEED" --authority-seed "$ECDSA_IDENTITY") || { fail "deploy: $address"; return; }
    problem=$(store_contract_call fresh-ecdsa-store "$SENDER_SEED" store check) || { fail "$problem"; return; }
    semantic_snapshot "$address" "$EV/ecdsa_store_state.txt" && grep -qx 'authority_threshold=1' "$EV/ecdsa_store_state.txt" \
        || { fail "unexpected committee: $(grep authority "$EV/ecdsa_store_state.txt" | tr '\n' ' ')"; return; }
    pass "contract at $address with a one-member ECDSA committee: deployed, stored a value, checked it"
}

check_ecdsa_mixed_committee_rotation() {
    local address rng problem log="$EV/ecdsa_rotation.log"
    address=$(registry_address fresh-ecdsa-store); rng=$(recalled "$(state_key fresh-ecdsa-store RNG)")
    [ -n "$address" ] || { skip "no contract with an ECDSA committee (G7-ecdsa-contract-committee)"; return; }
    tk_tx generate-txs contract-simple maintenance --funding-seed "$SENDER_SEED" --contract-address "$address" \
        --authority-seed "$ECDSA_IDENTITY" --new-authority-seed "$ECDSA_IDENTITY" --new-authority-seed "$RECEIVER_SEED" \
        --threshold 2 --counter 0 --rng-seed "$rng" -d "$NODE_WS" > "$log" 2>&1 || { fail "$(last_line_of "$log")"; return; }
    semantic_snapshot "$address" "$EV/ecdsa_store_rotated.txt" || { fail "the state is unreadable after the rotation"; return; }
    grep -qx 'authority_threshold=2' "$EV/ecdsa_store_rotated.txt" && grep -qx 'authority_counter=1' "$EV/ecdsa_store_rotated.txt" \
        || { fail "unexpected committee: $(grep authority "$EV/ecdsa_store_rotated.txt" | tr '\n' ' ')"; return; }
    problem=$(store_contract_call fresh-ecdsa-store "$SENDER_SEED" store) || { fail "after the rotation: $problem"; return; }
    pass "an ECDSA signature rotated the committee to one ECDSA and one Schnorr member, both required; the contract still works"
}

# --- Contract events --------------------------------------------------------------------------
# dapps/events.compact emits a Misc event (a name and a payload) and a ShieldedSpend event
# (a nullifier).
EVENT_NAME=$(as_32_bytes 'hard fork events')
EVENT_PAYLOAD=$(printf '%02x' {0..255})
EVENT_NULLIFIER=$(printf 'ee%.0s' {1..32})
events_address() { registry_address fresh-events; }

check_events_emitted() {
    local address emitted
    address=$(deploy_dapp events) || { fail "$address"; return; }
    emitted=$(dapp_file events_events.json)
    DAPP_EVENTS_OUT=1 dapp_call events "$SENDER_SEED" "$address" log "$EVENT_NAME" "$EVENT_PAYLOAD" || { fail "log(): $(dapp_err events log)"; return; }
    cp "$emitted" "$EV/events_of_log.json" 2> /dev/null || { fail "the toolkit wrote no events for log()"; return; }
    [ "$(jq length "$emitted")" = 1 ] && with_bytes_as_hex "$emitted" | grep -q "$EVENT_NAME" && with_bytes_as_hex "$emitted" | grep -q "$EVENT_PAYLOAD" \
        || { fail "log() should emit one event with the name and the payload: $(jq -c . "$emitted" | cut -c1-200)"; return; }
    DAPP_EVENTS_OUT=1 dapp_call events "$SENDER_SEED" "$address" mark "$EVENT_NULLIFIER" || { fail "mark(): $(dapp_err events mark)"; return; }
    cp "$emitted" "$EV/events_of_mark.json"
    with_bytes_as_hex "$emitted" | grep -q "$EVENT_NULLIFIER" || { fail "mark() did not emit the nullifier"; return; }
    pass "events dApp at $address: log() emitted one Misc event with a 32-byte name and a 256-byte payload, mark() one ShieldedSpend event"
}

count_events() {  # <contractEvents filter, GraphQL>
    gql_query "{ contractEvents(filter: { $1 }) { __typename } }" | jq '.data.contractEvents | length' 2> /dev/null
}

check_events_in_indexer() {
    local address problem answer name payload nullifier spends by_prefix in_last_call
    address=$(events_address)
    [ -n "$address" ] || { skip "no events dApp (G7-events-emitted)"; return; }
    problem=$(indexer_catch_up 180) || { fail "$problem"; return; }
    answer=$(gql_query "{ contractEvents(filter: { contractAddress: \"$address\" }) { __typename ... on MiscContractEvent { name payload } ... on ShieldedSpendEvent { nullifier } } }")
    echo "$answer" > "$EV/events_in_indexer.json"
    jq -e '.errors' <<< "$answer" > /dev/null 2>&1 && { fail "the indexer rejected the query: $(jq -c .errors <<< "$answer" | cut -c1-200)"; return; }
    name=$(jq -r '.data.contractEvents[] | select(.__typename == "MiscContractEvent") | .name' <<< "$answer" | head -1)
    payload=$(jq -r '.data.contractEvents[] | select(.__typename == "MiscContractEvent") | .payload' <<< "$answer" | head -1)
    nullifier=$(jq -r '.data.contractEvents[] | select(.__typename == "ShieldedSpendEvent") | .nullifier' <<< "$answer" | head -1)
    [ "$(jq '.data.contractEvents | length' <<< "$answer")" = 2 ] || { fail "the indexer has $(jq '.data.contractEvents | length' <<< "$answer") events for the dApp, expected 2"; return; }
    [ "${name#0x}" = "$EVENT_NAME" ] && [ "${payload#0x}" = "$EVENT_PAYLOAD" ] && [ "${nullifier#0x}" = "$EVENT_NULLIFIER" ] \
        || { fail "the indexer's name, payload or nullifier differ from the emitted bytes ($EV/events_in_indexer.json)"; return; }
    spends=$(count_events "contractAddress: \"$address\", types: [SHIELDED_SPEND]")
    by_prefix=$(count_events "contractAddress: \"$address\", types: [SHIELDED_SPEND], fieldPrefixes: [{ fieldName: \"nullifier\", prefix: \"${EVENT_NULLIFIER:0:8}\" }]")
    in_last_call=$(gql_query "{ contractAction(address: \"$address\") { ... on ContractCall { contractEvents { __typename } } } }" | jq '.data.contractAction.contractEvents | length' 2> /dev/null)
    [ "$spends" = 1 ] || { fail "filtering by type returned $spends events, expected 1"; return; }
    [ "$in_last_call" = 1 ] || { fail "the dApp's last call carries $in_last_call events, expected 1"; return; }
    [ "$by_prefix" = 1 ] || { fail "filtering by a nullifier prefix returned '$by_prefix' events, expected 1"; return; }
    pass "both events with the emitted bytes; filters by type and by nullifier prefix, and the events of one call, are exact"
}

check_events_in_midnightjs() {
    skip_without_client_stack && return
    local address count out="$EV/events_in_midnightjs"
    address=$(events_address)
    [ -n "$address" ] || { skip "no events dApp (G7-events-emitted)"; return; }
    client_run "$out" mjs_check.mjs events --address "$address" || { fail "$(client_error "$out")"; return; }
    count=$(jq -r .count "$out.json")
    if [ "${count:-0}" -ge 2 ]; then pass "queryContractEvents returns $count events: $(jq -r '[.events[].eventType] | join(", ")' "$out.json")"
    else fail "queryContractEvents returns $count events, expected at least 2"; fi
}

# --- ZKIR v3 ----------------------------------------------------------------------------------
# dapps/keccak.compact is compiled with --feature-zkir-v3 and hashes its argument with the
# native keccak256. The toolkit's embedded prover proves ZKIR v2 only (a toolkit limitation,
# not filed), so the proof is made at the proof server when the embedded prover refuses.
ZKIR3_PROVED_BY=""
check_zkir3_keccak_circuit() {
    local address preimage expected_digest result
    address=$(deploy_dapp keccak) || { fail "$address"; return; }
    preimage=$(printf 'a1%.0s' {1..32}); expected_digest=$(python3 "$FORK_DIR/dapps/keccak256.py" "$preimage")
    if DAPP_RESULT_OUT=1 dapp_call keccak "$SENDER_SEED" "$address" hash "$preimage"; then
        ZKIR3_PROVED_BY="the toolkit's embedded prover"
    else
        grep -q 'ir-source\[v3' "$(dapp_log keccak hash_prove)" 2> /dev/null || { fail "hash(): $(dapp_err keccak hash)"; return; }
        cp "$(dapp_log keccak hash_prove)" "$EV/zkir3_embedded_prover.log"
        proof_server_up new > /dev/null 2>&1 || { fail "the embedded prover refused ZKIR v3 and no $PROOF_SERVER_NEW_TAG proof server answers"; return; }
        PROOF_SERVER="$PROOF_SERVER_NEW_URL" DAPP_RESULT_OUT=1 dapp_call keccak "$SENDER_SEED" "$address" hash "$preimage" \
            || { fail "hash() proved at the proof server: $(dapp_err keccak hash)"; return; }
        ZKIR3_PROVED_BY="the $PROOF_SERVER_NEW_TAG proof server"
    fi
    result=$(dapp_file keccak_result.json)
    if [ -s "$result" ] && with_bytes_as_hex "$result" | grep -q "$expected_digest"; then
        pass "a ZKIR v3 proof from $ZKIR3_PROVED_BY verified on chain; keccak256 gives $expected_digest, as dapps/keccak256.py computes"
    else
        fail "the circuit's result does not contain $expected_digest: $(cut -c1-160 "$result" 2> /dev/null)"
    fi
}

# --- Cross-contract calls ---------------------------------------------------------------------
# dapps/ccc-outer.compact calls the events dApp.
check_cross_contract_deploy() {
    local callee address
    callee=$(events_address)
    [ -n "$callee" ] || { skip "needs the events dApp (G7-events-emitted)"; return; }
    # compact-js takes the callee's address as { bytes } or, in some releases, as bare hex.
    address=$(CCC_CALLEE_ADDRESS="$callee" deploy_dapp ccc-outer) \
        || address=$(CCC_CTOR_STYLE=hex CCC_CALLEE_ADDRESS="$callee" deploy_dapp ccc-outer) || { fail "$address"; return; }
    pass "the outer contract at $address deployed with the events contract as its callee"
}

# --- The cNIGHT -> NIGHT bridge -----------------------------------------------------------------
check_bridge_pallet_present() {
    local pallet found="" scripts
    for pallet in bridge c2mBridge; do
        if chain_check storage --pallet "$pallet" 2> "$EV/bridge_pallet.err" | jq -e '.present == true' > /dev/null 2>&1; then found=$pallet; break; fi
    done
    [ -n "$found" ] || { fail "no bridge pallet in the runtime: $(last_line_of "$EV/bridge_pallet.err")"; return; }
    scripts=$(chain_check storage --pallet "$found" --item mainChainScriptsConfiguration 2> /dev/null | jq -c '.value // null')
    if [ -n "$scripts" ] && [ "$scripts" != null ]; then pass "pallet '$found' is in the runtime; its main-chain scripts are set: $(cut -c1-160 <<< "$scripts")"
    else pass "pallet '$found' is in the runtime; no main-chain scripts are set, so the bridge stays inert until governance sets them"; fi
}

# Needs a network with a Cardano side, and in its settings: BRIDGE_TEST=1, BRIDGE_SIGNING_KEY,
# BRIDGE_ICS_CONFIG, OGMIOS_URL and BRIDGE_RECIPIENT_HEX (the spare wallet of the seeds file).
check_bridge_transfer() {
    [ "$TARGET" = network ] && [ "${BRIDGE_TEST:-0}" = 1 ] || { skip "a live transfer needs a network with a Cardano side and BRIDGE_TEST=1"; return; }
    local setting missing="" recipient_seed="${GENESIS_SEED[SPARE]}" before after waited=0
    for setting in BRIDGE_SIGNING_KEY BRIDGE_ICS_CONFIG OGMIOS_URL BRIDGE_RECIPIENT_HEX; do
        [ -n "${!setting:-}" ] || missing="$missing $setting"
    done
    [ -z "$missing" ] || { fail "BRIDGE_TEST=1, but the network settings lack:$missing"; return; }
    before=$(unshielded_night_of "$recipient_seed" bridge_recipient_before) || { fail "$before"; return; }
    tk bridge-transfer --signing-key "$BRIDGE_SIGNING_KEY" --ics-config "$BRIDGE_ICS_CONFIG" --ogmios-url "$OGMIOS_URL" \
        --recipient-address "$BRIDGE_RECIPIENT_HEX" --amount 5 > "$EV/bridge_transfer.log" 2>&1 || { fail "$(last_line_of "$EV/bridge_transfer.log")"; return; }
    while [ "$waited" -lt 1800 ]; do
        sleep 120; waited=$(( waited + 120 ))
        after=$(unshielded_night_of "$recipient_seed" bridge_recipient_after) || continue
        if big_gt "$after" "$before"; then pass "the recipient's NIGHT went $before -> $after"; return; fi
    done
    fail "no NIGHT was credited to the recipient within 30 minutes (still $before)"
}

# --- New parameters -----------------------------------------------------------------------------
fixed_point_parameter() {  # <name> <ledger parameters file>
    grep -iE "$1" "$2" 2> /dev/null | sed -nE 's/.*FixedPoint\(([0-9.eE+-]+)\).*/\1/p' | head -1
}

# Ledger 9 adds a minimum block price, which raises fees well above ledger 8's.
check_min_block_price() {
    local parameters="$EV/ledger_parameters.txt" minimum price_now price_before
    tk show-ledger-parameters -r "$NODE_WS" > "$parameters" 2>&1 && grep -q overall_price "$parameters" \
        || { fail "the ledger parameters are unreadable: $(last_line_of "$parameters")"; return; }
    minimum=$(fixed_point_parameter min_block_price "$parameters"); price_now=$(fixed_point_parameter overall_price "$parameters")
    price_before=$(fixed_point_parameter overall_price "$STATE_DIR/ledger_parameters_before_fork.txt")
    [ -n "$minimum" ] || { fail "there is no min_block_price after the fork"; return; }
    python3 -c "import sys; sys.exit(0 if float(sys.argv[1]) >= float(sys.argv[2]) else 1)" "$price_now" "$minimum" \
        || { fail "overall_price $price_now is below min_block_price $minimum"; return; }
    pass "min_block_price is $minimum; overall_price was ${price_before:-unrecorded} before the fork and is $price_now now"
}

bullet "Every new feature: ECDSA signatures"
check ecdsa-wallet-spends              "an ECDSA identity has its own address, is funded and spends with an ECDSA signature" check_ecdsa_wallet_spends
check ecdsa-contract-committee         "a contract with an ECDSA maintenance committee: deploy, store, check"              check_ecdsa_contract_committee
check ecdsa-mixed-committee-rotation   "an ECDSA-signed rotation to a committee of one ECDSA and one Schnorr member"       check_ecdsa_mixed_committee_rotation

bullet "Every new feature: contract events"
check events-emitted                   "a dApp emits a Misc and a ShieldedSpend event"                                     check_events_emitted
check events-in-indexer                "the indexer serves both events with the emitted bytes, and filters them"           check_events_in_indexer
check events-in-midnightjs             "Midnight.js returns the dApp's events"                                             check_events_in_midnightjs

bullet "Every new feature: ZKIR v3"
check zkir3-keccak-circuit             "a keccak256 circuit compiled for ZKIR v3 deploys, proves and verifies"             check_zkir3_keccak_circuit

bullet "Every new feature: cross-contract calls"
check cross-contract-deploy            "a contract that calls another contract deploys"                                    check_cross_contract_deploy
not_covered cross-contract-call        "the outer contract calls the events contract; events from two addresses in one transaction" "toolkit-js cannot execute a cross-contract call: it takes one on-chain state and has no way to pass the callee's (not filed)"

bullet "Every new feature: the cNIGHT -> NIGHT bridge"
check bridge-pallet-present            "the bridge pallet is in the runtime"                                               check_bridge_pallet_present
check bridge-transfer                  "a cNIGHT -> NIGHT transfer is credited"                                            check_bridge_transfer

bullet "New parameters"
check min-block-price                  "a minimum block price is set and the price respects it"                            check_min_block_price

bullet "The gaps: features nothing covers yet"
not_covered rsa-verification           "RSA signature verification inside a circuit"                                       "no contract with an RSA-1024/2048 verification circuit and a signed test vector yet"
not_covered usdcx-hashing              "the USDCx hashing requirements, end to end"                                        "beyond native keccak256 (G7-zkir3-keccak-circuit), the USDCx hashing vectors are not wired into a contract"
