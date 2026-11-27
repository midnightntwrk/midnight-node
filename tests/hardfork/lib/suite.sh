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

# What every group script starts from: the libraries, the fork's known issues, and the
# helpers that say which side of the fork a script works on and what it needs to run.

_SUITE_LIB="$(dirname "${BASH_SOURCE[0]}")"
source "$_SUITE_LIB/target.sh"
source "$_SUITE_LIB/results.sh"
source "$_SUITE_LIB/chain.sh"
source "$_SUITE_LIB/dapps.sh"
source "$_SUITE_LIB/clients.sh"
source "$_SUITE_LIB/wallets.sh"
source "$_SUITE_LIB/funded_dapps.sh"
# shellcheck source=/dev/null
[ ! -s "$FORK_DIR/known_issues.sh" ] || source "$FORK_DIR/known_issues.sh"
require_cmd docker jq curl python3 xxd git || exit 2

# --- Which side of the fork ---------------------------------------------------------
# "old" is the ledger the chain forks from, "new" the one it forks to. Everything that
# differs between the two is set here, so a check reads the same on both sides.
use_ledger_side() {  # <old|new>
    SIDE="$1"
    use_toolkit "$SIDE"
    if [ "$SIDE" = old ]; then
        LEDGER=$OLD_LEDGER
        EXPECTED_SPEC=$OLD_EXPECTED_SPEC
        EXPECTED_LEDGER_VERSION=$OLD_EXPECTED_LEDGER
        EXPECTED_NODE_VERSION=$OLD_EXPECTED_NODE_VERSION
        EXPECTED_COMPACTC=$OLD_EXPECTED_COMPACTC
        PROOF_SERVER_URL=$PROOF_SERVER_OLD_URL
        PROOF_SERVER_TAG=$PROOF_SERVER_OLD_TAG
        export DAPP_OUT=out
    else
        LEDGER=$NEW_LEDGER
        EXPECTED_SPEC=$NEW_EXPECTED_SPEC
        EXPECTED_LEDGER_VERSION=$NEW_EXPECTED_LEDGER
        EXPECTED_NODE_VERSION=$NEW_EXPECTED_NODE_VERSION
        EXPECTED_COMPACTC=$NEW_EXPECTED_COMPACTC
        PROOF_SERVER_URL=$PROOF_SERVER_NEW_URL
        PROOF_SERVER_TAG=$PROOF_SERVER_NEW_TAG
        export DAPP_OUT=out-new
    fi
    SDK_ERA="v$LEDGER"   # how the wallet SDK and Midnight.js name this ledger
}

# --- What a group script needs before it can run --------------------------------------
# Each of these ends the script with exit 3 and a row that says why.
evidence_dir() {  # <name> -> sets EV, the directory this script saves raw output to
    EV="$EVIDENCE_DIR/$1"
    mkdir -p "$EV"
}

require_chain_on() {  # <old|new>
    local spec; spec=$(spec_version)
    [ -n "$spec" ] || cannot_run "the node at $NODE_HTTP does not answer"
    if [ "$1" = old ]; then
        [ "$spec" -lt "$NEW_SPEC_FLOOR" ] || cannot_run "the chain is on spec $spec: it has forked already"
    else
        [ "$spec" -ge "$NEW_SPEC_FLOOR" ] || cannot_run "the chain is on spec $spec: it has not forked yet"
    fi
}

require_indexer() {
    indexer_resolve --probe || cannot_run "no indexer answers at $INDEXER_BASE"
}

require_group2_record() {
    [ -n "${SPEC_BEFORE_FORK:-}" ] || cannot_run "nothing recorded before the fork: run group 2 first"
}

require_fork_block() {  # sets FORK_HEIGHT
    FORK_HEIGHT=$(fork_height "${SPEC_BEFORE_FORK:-$OLD_EXPECTED_SPEC}") \
        || cannot_run "the fork block cannot be verified (the RPC node must keep old runtime state)"
}

require_clients() {
    clients_ready > "$EV/clients_ready.log" 2>&1 || cannot_run "npm ci in $CLIENT_PACKAGES_DIR failed ($EV/clients_ready.log)"
}

# What every group after the fork starts with.
start_after_fork() {  # <evidence dir name>
    evidence_dir "$1"
    recall_state
    use_ledger_side new
    use_wallets genesis
    require_indexer
    require_chain_on new
    require_group2_record
    require_fork_block
}

# --- The fork's own checks --------------------------------------------------------------
# A fork adds to a group with a file of the same name in its directory:
# forks/<fork>/g07_new_ledger_features.sh adds to groups/g07_new_ledger_features.sh.
fork_has_checks() {
    [ -s "$FORK_DIR/$(basename "$0")" ]
}
run_fork_checks() {
    # shellcheck source=/dev/null
    ! fork_has_checks || source "$FORK_DIR/$(basename "$0")"
}

# --- Steps many checks share -------------------------------------------------------------
# These return 0 or 1 and print what went wrong; the check decides the verdict.

send_night() {  # <log name> <toolkit single-tx arguments...>
    local log="$EV/$1.log"; shift
    tk_tx generate-txs single-tx "$@" -d "$NODE_WS" > "$log" 2>&1 && return 0
    last_line_of "$log"
    return 1
}

# The built-in "store" contract of the toolkit: it stores a value and checks it.
store_contract_deploy() {  # <label> <funding seed> [deploy arguments...] -> address
    local label=$1 seed=$2 rng tx_file log address
    shift 2
    tx_file="$EV/deploy_$label.mn"; log="$EV/$label.log"
    rng=$(openssl rand -hex 32)
    tk_chain generate-txs --dest-file "$tx_file" contract-simple deploy --funding-seed "$seed" --rng-seed "$rng" "$@" > "$log" 2>&1 \
        || { last_line_of "$log"; return 1; }
    tk_send "$tx_file" >> "$log" 2>&1 || { last_line_of "$log"; return 1; }
    address=$(tk contract-address --src-file "$tx_file" 2>/dev/null | tail -1)
    is_hex64 "$address" || { echo "no contract address in $tx_file"; return 1; }
    registry_add "$label" store "$address" "$SIDE"
    remember "$(state_key "$label" RNG)" "$rng"
    echo "$address"
}

store_contract_call() {  # <label> <funding seed> <call: store|check>...
    local label=$1 seed=$2 address rng call log
    shift 2
    address=$(registry_address "$label"); rng=$(recalled "$(state_key "$label" RNG)"); log="$EV/$label.log"
    [ -n "$address" ] && [ -n "$rng" ] || { echo "no contract $label recorded"; return 1; }
    for call in "$@"; do
        tk_tx generate-txs contract-simple call --funding-seed "$seed" --call-key "$call" --rng-seed "$rng" \
            --contract-address "$address" -d "$NODE_WS" >> "$log" 2>&1 || { echo "$call: $(last_line_of "$log")"; return 1; }
    done
}

# Replace a store contract's maintenance committee. The contract's counter goes up by one.
store_contract_rotate_authority() {  # <label> <fee payer seed> <signer seed> <new member seed>...
    local label=$1 payer=$2 signer=$3 address rng counter member new_members=() log
    shift 3
    address=$(registry_address "$label"); rng=$(recalled "$(state_key "$label" RNG)")
    counter=$(recalled "$(state_key "$label" AUTHORITY_COUNTER)"); counter=${counter:-0}
    log="$EV/${label}_rotate.log"
    for member in "$@"; do new_members+=(--new-authority-seed "$member"); done
    tk_tx generate-txs contract-simple maintenance --funding-seed "$payer" --authority-seed "$signer" "${new_members[@]}" \
        --threshold 1 --counter "$counter" --contract-address "$address" --rng-seed "$rng" -d "$NODE_WS" > "$log" 2>&1 \
        || { last_line_of "$log"; return 1; }
    semantic_snapshot "$address" "$EV/${label}_rotated.txt" || { echo "the state is unreadable after the rotation"; return 1; }
    grep -qx "authority_counter=$(( counter + 1 ))" "$EV/${label}_rotated.txt" \
        || { echo "expected counter $(( counter + 1 )), the contract says $(grep authority_counter "$EV/${label}_rotated.txt")"; return 1; }
    remember "$(state_key "$label" AUTHORITY_COUNTER)" "$(( counter + 1 ))"
}

# Every block from genesis: a fresh cache on local-env, the standing cache on a network.
fetch_every_block() {  # <log>
    if [ "$TARGET" = local ]; then
        rm -rf "$EV/fetch_cache"; mkdir -p "$EV/fetch_cache"
        tk fetch --fetch-cache "redb:$EV/fetch_cache/fetch.db" -s "$NODE_WS" > "$1" 2>&1
    else
        tk_chain_ro fetch > "$1" 2>&1
    fi
}

# Entry points and maintenance authority of a contract, as group 2 recorded them.
recorded_contract_state() { echo "$STATE_DIR/contract_state/$1.txt"; }   # <address>
record_contract_state() {  # <address>
    mkdir -p "$STATE_DIR/contract_state"
    semantic_snapshot "$1" "$(recorded_contract_state "$1")"
}

# --- Checks more than one group runs -------------------------------------------------------
check_chain_healthy() {  # [fewest blocks expected in 60 s]
    local report blocks
    report=$(chain_health 60) || { fail "$report"; return; }
    blocks=$(grep -oE '\(\+[0-9]+\)' <<< "$report" | head -1 | tr -dc 0-9)
    if [ "${blocks:-0}" -ge "${1:-1}" ]; then pass "$report"
    else warn "$report: fewer blocks than 6-second slots predict"; fi
}

check_indexer_at_tip() {
    local problem
    if problem=$(indexer_catch_up 180); then pass "indexer at #$(indexer_height), node finalized #$(finalized_height)"
    else fail "$problem"; fi
}

# Node and indexer must serve the same state bytes for every contract the suite deployed.
check_node_and_indexer_agree_on_state() {
    local address label difference total=0 different="" stale=""
    for address in $(registry_addresses); do
        total=$(( total + 1 )); label=$(registry_label "$address")
        difference=$(contract_state_node_vs_indexer "$address" "$EV/node_vs_indexer_$address") && continue
        if known_indexer_stale_state "$address" "$EV/node_vs_indexer_$address" "${FORK_HEIGHT:-}"; then stale="$stale $label"
        else different="$different $label($difference)"; fi
    done
    if [ "$total" = 0 ]; then skip "no contracts recorded"
    elif [ -n "$different" ]; then fail "different state for:$different"
    elif [ -n "$stale" ]; then known "the indexer serves the last pre-fork state, data unchanged, for contracts untouched since the fork ($KNOWN_INDEXER_STALE_STATE_ISSUE):$stale"
    else pass "$total contracts: identical state bytes from node and indexer"; fi
}

# Overridden by a fork's known_issues.sh when it has such an issue.
if ! declare -F known_indexer_stale_state > /dev/null; then
    known_indexer_stale_state() { return 1; }
fi
