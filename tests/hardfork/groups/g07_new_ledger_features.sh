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

# Group 7: new-ledger features after the fork.
#
# Does everything from group 2 work again from fresh state, and does every new feature work?
#
# The regression matrix is the feature checks of group 2 (ledger_features.sh), run on the new
# ledger with wallets that never transacted before and with newly deployed contracts and
# dApps. The fork adds one check for each thing the new ledger introduces, its new
# parameters, and the features nothing covers yet (forks/<fork>/<this file>).
set -e
source "$(dirname "${BASH_SOURCE[0]}")/../lib/suite.sh"
source "$(dirname "${BASH_SOURCE[0]}")/ledger_features.sh"

start_after_fork g7
require_clients
# The wallets of the seeds file fund the fresh ones and pay for their DUST registration.
FUNDER_SEED="$SENDER_SEED"
REGISTRATION_PAYER[SENDER]="$RECEIVER_SEED"
REGISTRATION_PAYER[RECEIVER]="$HOLDER_SEED"
use_wallets fresh
export DAPP_FILE_PREFIX=fresh
CONTRACT_PREFIX="fresh-"
# Enough that the DUST it generates within a minute pays for deploys on the new ledger.
FRESH_NIGHT_PER_UTXO=$(( 100000 * NIGHT ))
FRESH_SHIELDED_NIGHT=1000

group_start G7 "New-ledger features after the fork, on $NETWORK_NAME (ledger $NEW_LEDGER, toolkit $TK_IMAGE)"

# --- Checks only group 7 runs -------------------------------------------------------------
check_wallets_funded() {
    local problem night shielded
    problem=$(send_night fund_fresh_sender_shielded --source-seed "$FUNDER_SEED" --shielded-amount "$FRESH_SHIELDED_NIGHT" \
        --destination-address "$SENDER_SHIELDED_ADDRESS") || { fail "shielded NIGHT for the fresh sender: $problem"; return; }
    wallet_snapshot "$SENDER_SEED" "$EV/fresh_sender_wallet.json" || { fail "the fresh sender's wallet is unreadable"; return; }
    night=$(wallet_night_unshielded "$EV/fresh_sender_wallet.json"); shielded=$(wallet_night_shielded "$EV/fresh_sender_wallet.json")
    big_gt "$night" 0 && big_gt "$shielded" 0 || { fail "the fresh sender holds unshielded $night and shielded $shielded"; return; }
    pass "the fresh sender holds unshielded NIGHT $night and shielded NIGHT $shielded; it never transacted before the fork"
}

check_all_nodes_on_new_release() {
    local versions on_another_version
    versions=$(node_versions)
    on_another_version=$(grep -vF "=$NEW_EXPECTED_NODE_VERSION" <<< "$versions" | cut -d= -f1 | xargs)
    if [ -z "$versions" ]; then fail "no node answers"
    elif [ -n "$on_another_version" ]; then fail "not on $NEW_EXPECTED_NODE_VERSION: $on_another_version (${versions//$'\n'/, })"
    else pass "${versions//$'\n'/, }"; fi
}

# A dApp developer after the fork: compile with the compiler for the new Midnight.js, deploy
# and call through it.
check_midnightjs_deploys_and_calls_counter() {
    skip_without_client_stack && return
    local artefacts address out="$EV/midnightjs_counter"
    artefacts="$(dapp_dir counter)/out-mjs-new"
    midnightjs_compile counter out-mjs-new "$MIDNIGHTJS_NEW_ARTEFACTS_COMPACTC" > "$EV/midnightjs_compile_counter.log" 2>&1 \
        || { fail "compactc $MIDNIGHTJS_NEW_ARTEFACTS_COMPACTC failed: $(dapp_dir counter)/compile.out-mjs-new.log"; return; }
    client_run "${out}_deploy" mjs_check.mjs deploy --contract-dir "$artefacts" --name counter || { fail "deploy: $(client_error "${out}_deploy")"; return; }
    address=$(jq -r .contractAddress "${out}_deploy.json")
    registry_add fresh-midnightjs-counter dapp "$address" new
    if client_run "${out}_call" mjs_check.mjs call --contract-dir "$artefacts" --name counter --address "$address" --times 2; then
        pass "deployed at $address, two increments: $(jq -r '[.results[].status] | join(", ")' "${out}_call.json")"
    else fail "deployed at $address, but the call failed: $(client_error "${out}_call")"; fi
}

# bboard has a witness and an assertion: a second post must be refused before anything is proved.
check_midnightjs_bboard_flow() {
    skip_without_client_stack && return
    local artefacts address out="$EV/midnightjs_bboard"
    artefacts="$(dapp_dir bboard)/out-mjs-new"
    midnightjs_compile bboard out-mjs-new "$MIDNIGHTJS_NEW_ARTEFACTS_COMPACTC" > "$EV/midnightjs_compile_bboard.log" 2>&1 \
        || { fail "compactc $MIDNIGHTJS_NEW_ARTEFACTS_COMPACTC failed: $(dapp_dir bboard)/compile.out-mjs-new.log"; return; }
    client_run "${out}_deploy" mjs_check.mjs deploy --contract-dir "$artefacts" --name bboard || { fail "deploy: $(client_error "${out}_deploy")"; return; }
    address=$(jq -r .contractAddress "${out}_deploy.json")
    registry_add fresh-midnightjs-bboard dapp "$address" new
    client_run "${out}_post" mjs_check.mjs call --contract-dir "$artefacts" --name bboard --address "$address" --circuit post --args '["first post"]' \
        || { fail "post: $(client_error "${out}_post")"; return; }
    client_run "${out}_second_post" mjs_check.mjs call --contract-dir "$artefacts" --name bboard --address "$address" --circuit post --args '["second post"]' \
        --expect-error "occupied board" || { fail "a second post was not refused as expected: $(client_error "${out}_second_post")"; return; }
    client_run "${out}_take_down" mjs_check.mjs call --contract-dir "$artefacts" --name bboard --address "$address" --circuit take_down \
        || { fail "take_down: $(client_error "${out}_take_down")"; return; }
    pass "deployed at $address; a post landed; a second post was refused by the contract's assertion; take_down with the poster's key landed"
}

# --- The group, in runbook order ------------------------------------------------------------
REGRESSION_BULLET="The regression matrix: everything from group 2 again, from fresh state"
bullet "$REGRESSION_BULLET"
check dust-registration           "fresh wallets get NIGHT and register for DUST"                         check_dust_registration "$FUNDER_SEED" "$FRESH_NIGHT_PER_UTXO" SENDER RECEIVER
check wallets-funded              "the fresh sender holds unshielded and shielded NIGHT"                  check_wallets_funded
check dust-generating             "DUST is generating for the fresh sender and receiver"                  check_dust_generating SENDER RECEIVER

run_version_checks
check all-nodes-on-new-release    "every node we can ask runs $NEW_EXPECTED_NODE_VERSION"                 check_all_nodes_on_new_release
run_transfer_checks
run_contract_checks
run_dapp_checks
run_client_stack_checks
check midnightjs-deploys-and-calls-counter "a counter deployed and called through Midnight.js"           check_midnightjs_deploys_and_calls_counter
check midnightjs-bboard-flow      "bboard through Midnight.js: post, a refused second post, take down"    check_midnightjs_bboard_flow
run_indexer_checks

run_fork_checks

group_finish
