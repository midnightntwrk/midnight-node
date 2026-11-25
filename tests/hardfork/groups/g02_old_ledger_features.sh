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

# Group 2: old-ledger features before the fork.
#
# Does every major feature work on the chain that is about to fork? This proves the starting
# point is healthy, creates the state the fork must preserve, and leaves things half-done on
# purpose for the groups after the fork: two unfinished games, a saved wallet state, one
# old-ledger transaction built but not sent, and a wallet ready to transact across the fork.
#
# The fork adds the checks of its new tooling on the old chain (forks/<fork>/<this file>).
set -e
source "$(dirname "${BASH_SOURCE[0]}")/../lib/suite.sh"
source "$(dirname "${BASH_SOURCE[0]}")/ledger_features.sh"

evidence_dir g2
recall_state
use_ledger_side old
use_wallets genesis
require_indexer
require_chain_on old
require_clients
export DAPP_FILE_PREFIX=prefork
CONTRACT_PREFIX=""
remember SPEC_BEFORE_FORK "$(spec_version)"

# Enough NIGHT that the DUST it generates in a minute on the new ledger pays a fee there.
CROSSER_NIGHT_PER_UTXO=$(( 10000 * NIGHT ))
REGISTRATION_PAYER[CROSSER]="$RECEIVER_SEED"

group_start G2 "Old-ledger features before the fork, on $NETWORK_NAME (ledger $OLD_LEDGER, toolkit $TK_IMAGE)"

# --- Checks only group 2 runs -----------------------------------------------------------
check_wallets_funded() {
    local role seed night dust_total dust_sources report="" problems=""
    for role in SENDER RECEIVER HOLDER SPARE; do
        seed="${role}_SEED"
        wallet_snapshot "${!seed}" "$EV/wallet_$role.json" || { problems="$problems $role: wallet unreadable;"; continue; }
        dust_snapshot "${!seed}" "$EV/dust_$role.json" || { problems="$problems $role: DUST balance unreadable;"; continue; }
        night=$(wallet_night_unshielded "$EV/wallet_$role.json")
        dust_total=$(jq -r .total "$EV/dust_$role.json"); dust_sources=$(jq -r '.source | length' "$EV/dust_$role.json")
        report="$report $role: NIGHT $night, $dust_sources DUST sources;"
        [ "$role" = SPARE ] && continue   # the spare only receives
        big_gt "$night" 0 || problems="$problems $role has no unshielded NIGHT;"
        big_gt "$dust_total" 0 || problems="$problems $role has no DUST to pay fees;"
    done
    if [ -z "$problems" ]; then pass "$report"
    else fail "$problems ($report)"; fi
}

check_genesis_hash() {
    local hash; hash=$(genesis_hash)
    remember GENESIS_HASH "$hash"
    if [ -n "${EXPECTED_GENESIS_HASH:-}" ] && [ "$hash" != "$EXPECTED_GENESIS_HASH" ]; then
        fail "genesis $hash, the network settings expect $EXPECTED_GENESIS_HASH: was the network reset?"
    else pass "genesis $hash"; fi
}

check_ledger_parameters_recorded() {
    if tk show-ledger-parameters -r "$NODE_WS" > "$EV/ledger_parameters.txt" 2>&1 && grep -q overall_price "$EV/ledger_parameters.txt"; then
        cp "$EV/ledger_parameters.txt" "$STATE_DIR/ledger_parameters_before_fork.txt"
        pass "$(grep -c ':' "$EV/ledger_parameters.txt") lines kept in state/ledger_parameters_before_fork.txt"
    else fail "$(last_line_of "$EV/ledger_parameters.txt")"; fi
}

# The fork may reset the DUST generation tree; group 5 compares against these.
check_indexer_tip_recorded() {
    local tip protocol_version dust_tree_end
    tip=$(indexer_block)
    protocol_version=$(jq -r '.protocolVersion // empty' <<< "$tip"); dust_tree_end=$(jq -r '.dustGenerationEndIndex // empty' <<< "$tip")
    [ -n "$protocol_version" ] || { fail "the indexer's block query is incomplete: $tip"; return; }
    [ "${dust_tree_end:-0}" -gt 0 ] || { fail "dustGenerationEndIndex is '$dust_tree_end': the DUST generation tree is empty, so there is nothing to compare after the fork"; return; }
    remember INDEXER_PROTOCOL_VERSION_BEFORE_FORK "$protocol_version"
    remember DUST_TREE_END_BEFORE_FORK "$dust_tree_end"
    pass "protocolVersion $protocol_version, dustGenerationEndIndex $dust_tree_end"
}

# Entry points and maintenance authority of everything deployed above: the fork must keep them.
check_contract_states_recorded() {
    local address total=0 unreadable=""
    for address in $(registry_addresses old); do
        total=$(( total + 1 ))
        record_contract_state "$address" || unreadable="$unreadable $(registry_label "$address")"
    done
    if [ "$total" = 0 ]; then fail "no contract was deployed"
    elif [ -n "$unreadable" ]; then fail "unreadable:$unreadable"
    else pass "$total contracts recorded in state/contract_state/"; fi
}

# Before the fork a dApp on the new Midnight.js reaches old-ledger contracts with artefacts
# from the compiler for that ledger.
check_midnightjs_calls_counter() {
    skip_without_client_stack && return
    local counter out="$EV/midnightjs_counter"
    counter=$(registry_address counter)
    [ -n "$counter" ] || { skip "no counter was deployed (G2-dapp-counter)"; return; }
    midnightjs_compile counter out-mjs "$MIDNIGHTJS_OLD_ARTEFACTS_COMPACTC" for-old-ledger > "$EV/midnightjs_compile.log" 2>&1 \
        || { fail "compactc $MIDNIGHTJS_OLD_ARTEFACTS_COMPACTC failed: $(dapp_dir counter)/compile.out-mjs.log"; return; }
    MN_ZK_VERIFY=warn client_run "$out" mjs_check.mjs retained-call --contract-dir "$(dapp_dir counter)/out-mjs" \
        --name counter --address "$counter" --circuit increment --times 2 || { fail "$(client_error "$out")"; return; }
    remember MIDNIGHTJS_COUNT_BEFORE_FORK "$(jq -r '.privateState.count // empty' "$out.json")"
    remember MIDNIGHTJS_TRANSACTION_BEFORE_FORK "$(jq -r '.results[-1].txId // empty' "$out.json")"
    pass "two increments on $counter: $(jq -r '[.results[].status] | join(", ")' "$out.json"); private count $MIDNIGHTJS_COUNT_BEFORE_FORK"
}

check_old_wallet_works() {
    local out="$EV/old_wallet_send"
    if old_wallet_send "$out"; then
        pass "wallet SDK $(old_wallet_sdk_version) synced in $(jq -r .syncMs "$out.json") ms and sent a transfer, proved at the $PROOF_SERVER_OLD_TAG proof server"
    else fail "wallet SDK from before the fork: $(client_error "$out")"; fi
}

check_faucet_answers() {
    [ "$TARGET" = network ] || { skip "local-env has no faucet"; return; }
    [ -n "${FAUCET_URL:-}" ] || { skip "no FAUCET_URL in the network settings"; return; }
    local status; status=$(curl -s -m 15 -o /dev/null -w '%{http_code}' "$FAUCET_URL")
    if [ "$status" = 200 ]; then pass "$FAUCET_URL answers 200 (a probe: no payout is requested)"
    else warn "$FAUCET_URL answers $status"; fi
}

check_new_toolkit_replays_chain() {
    use_toolkit new
    require_image "$TK_IMAGE" 2>/dev/null || { fail "image $TK_IMAGE is missing"; use_ledger_side old; return; }
    if tk_chain_ro -q show-wallet --seed "$SENDER_SEED" > "$EV/new_toolkit_replay.log" 2>&1; then
        pass "$TK_IMAGE replayed $(node_height) blocks of runtime $(spec_version)"
    else
        fail "$(grep -m1 -o 'UnsupportedBlockVersion([0-9]*)' "$EV/new_toolkit_replay.log" || last_line_of "$EV/new_toolkit_replay.log")"
    fi
    use_ledger_side old
}

check_game_left_half_revealed() {
    local address problem
    address=$(game_deploy half-revealed) || { fail "$address"; return; }
    problem=$(game_start half-revealed "$ROCK" "$PAPER") || { fail "$problem"; return; }
    problem=$(game_call half-revealed host reveal) || { fail "$problem"; return; }
    problem=$(contract_must_hold_night "$address" $(( GAME_STAKE * 2 ))) || { fail "$problem"; return; }
    pass "game at $address: both staked, the host revealed rock, the guest's paper stays committed; $(( GAME_STAKE * 2 )) NIGHT held"
}

check_game_left_waiting() {
    local address problem
    address=$(game_deploy waiting) || { fail "$address"; return; }
    game_choose_move waiting host "$SCISSORS"
    problem=$(game_pay_stake waiting host host_game) || { fail "$problem"; return; }
    problem=$(contract_must_hold_night "$address" "$GAME_STAKE") || { fail "$problem"; return; }
    pass "game at $address: the host staked and committed, nobody joined; $GAME_STAKE NIGHT held"
}

SDK_WALLET_STATE="$STATE_DIR/sdk_wallet_state_before_fork.json"
check_sdk_wallet_state_saved() {
    skip_without_client_stack && return
    local out="$EV/sdk_wallet_state_saved"
    if client_run "$out" wallet_check.mjs save --out "$SDK_WALLET_STATE"; then
        pass "the sender's wallet state serialized at protocol version $(jq -r .protocolVersion "$out.json"), as an application persists it"
    else fail "$(client_error "$out")"; fi
}

# The node's own verdict on it after the fork is group 8's. Until then the holder must not
# spend, or the transaction's inputs are gone.
check_unsent_transaction_saved() {
    local tx_file="$STATE_DIR/unsent_old_transaction.mn" extrinsic verdict holder_night
    rm -f "$STATE_DIR/unsent_old_transaction.extrinsic"
    tk_chain generate-txs --dest-file "$tx_file" single-tx --source-seed "$HOLDER_SEED" --unshielded-amount 3 \
        --destination-address "$RECEIVER_ADDRESS" > "$EV/unsent_transaction.log" 2>&1 && [ -s "$tx_file" ] \
        || { fail "$(last_line_of "$EV/unsent_transaction.log")"; return; }
    extrinsic=$(bare_extrinsic "$tx_file") || { fail "no Midnight transaction in $tx_file"; return; }
    verdict=$(validate_extrinsic "$extrinsic")
    [[ "$verdict" == 0x00* ]] || { fail "the node does not accept it on ledger $OLD_LEDGER: validate_transaction says $verdict"; return; }
    echo "$extrinsic" > "$STATE_DIR/unsent_old_transaction.extrinsic"
    wallet_snapshot "$HOLDER_SEED" "$EV/holder_wallet.json" || { fail "the holder's wallet is unreadable"; return; }
    holder_night=$(wallet_night "$EV/holder_wallet.json")
    remember HOLDER_NIGHT_BEFORE_FORK "$holder_night"
    pass "valid at the node and saved unsent; the holder has $holder_night NIGHT (unshielded, shielded) and must not spend before the fork"
}

check_crosser_wallet_ready() {
    skip_without_client_stack && return
    local out="$EV/crosser_sdk_sync" night dust_coins
    CLIENT_SEED="$CROSSER_SEED" client_run "$out" wallet_check.mjs sync || { fail "$(client_error "$out")"; return; }
    night=$(sdk_unshielded_balance "$out.json"); dust_coins=$(jq -r '.wallet.dust.availableCoins // 0' "$out.json")
    big_gt "$night" 0 || { fail "the wallet SDK sees no NIGHT for the crosser"; return; }
    if [ "$dust_coins" -gt 0 ]; then pass "the wallet SDK sees the crosser's NIGHT $night and $dust_coins DUST coins"
    else warn "the wallet SDK sees NIGHT $night but no DUST coin yet: the crosser's first transfers may fail until DUST accrues"; fi
}

# --- The group, in runbook order ----------------------------------------------------------
run_version_checks
check genesis-hash                "the genesis hash, as the network settings name it"                   check_genesis_hash

bullet "DUST: registration, generation, paying fees with it"
check wallets-funded              "sender, receiver and holder have NIGHT and DUST"                     check_wallets_funded
check dust-registration           "a newly funded wallet, the crosser, registers for DUST"              check_dust_registration "$SENDER_SEED" "$CROSSER_NIGHT_PER_UTXO" CROSSER
check dust-generating             "DUST is generating for the sender and for the crosser"               check_dust_generating SENDER CROSSER

run_transfer_checks
run_contract_checks
run_dapp_checks
run_client_stack_checks
check midnightjs-calls-counter    "Midnight.js calls the counter with old-ledger artefacts"             check_midnightjs_calls_counter
check old-wallet-works            "the wallet SDK release from before the fork syncs and sends"         check_old_wallet_works

bullet "The Cardano side: the bridge, cNIGHT observation, governance sync and committee rotation"
not_covered cardano-side          "the Cardano side before the fork"                                    "no check reads the Cardano side on the old ledger"

bullet "The faucet pays out"
check faucet-answers              "the faucet answers"                                                  check_faucet_answers

bullet "The new tooling on the old chain"
check new-toolkit-replays-chain   "the new toolkit replays this old-ledger chain"                       check_new_toolkit_replays_chain
run_fork_checks

bullet "Left behind for the groups after the fork"
check game-left-half-revealed     "a game left with both stakes in and only the host revealed"          check_game_left_half_revealed
check game-left-waiting           "a game left waiting for a guest, the host's stake in"                check_game_left_waiting
check sdk-wallet-state-saved      "the wallet SDK's state saved, as an application persists it"         check_sdk_wallet_state_saved
check unsent-transaction-saved    "an old-ledger transaction built, validated and saved unsent"         check_unsent_transaction_saved
check crosser-wallet-ready        "the wallet SDK sees the crosser's NIGHT and DUST"                    check_crosser_wallet_ready

run_indexer_checks

bullet "Recorded for the groups after the fork"
check contract-states-recorded    "entry points and authority of every contract deployed above"         check_contract_states_recorded
check ledger-parameters-recorded  "the ledger parameters"                                               check_ledger_parameters_recorded
check indexer-tip-recorded        "the indexer's protocol version and DUST generation tree"             check_indexer_tip_recorded

group_finish
