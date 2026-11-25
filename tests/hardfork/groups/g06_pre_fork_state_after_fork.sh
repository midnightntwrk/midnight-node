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

# Group 6: pre-fork state still works after the fork.
#
# Can users still spend, call and finish what they had before the fork? Everything here uses
# the wallets, contracts and dApps of group 2. It runs before group 7, so fresh activity does
# not blur what the fork preserved.
set -e
source "$(dirname "${BASH_SOURCE[0]}")/../lib/suite.sh"

start_after_fork g6
require_clients
export DAPP_FILE_PREFIX=prefork
CONTRACT_PREFIX=""
SENDER_COIN_PUBLIC=$(wallet_address "$SENDER_SEED" coin-public)

group_start G6 "Pre-fork state still works after the fork, on $NETWORK_NAME (fork block #$FORK_HEIGHT, toolkit $TK_IMAGE)"

# --- Pre-fork balances can be spent ---------------------------------------------------------
check_transfer_unshielded() {
    local problem
    problem=$(send_night transfer_unshielded --source-seed "$SENDER_SEED" --unshielded-amount 10 \
        --destination-address "$SPARE_ADDRESS") && pass "landed" || fail "$problem"
}

check_transfer_shielded() {
    local problem
    problem=$(send_night transfer_shielded --source-seed "$SENDER_SEED" --shielded-amount 10 \
        --destination-address "$SPARE_SHIELDED_ADDRESS") && pass "landed" || fail "$problem"
}

check_transfer_mixed() {
    local problem
    problem=$(send_night transfer_mixed --source-seed "$RECEIVER_SEED" --shielded-amount 5 --unshielded-amount 5 \
        --destination-address "$SPARE_SHIELDED_ADDRESS" --destination-address "$SPARE_ADDRESS") && pass "landed" || fail "$problem"
}

check_transfer_two_destinations() {
    local problem
    problem=$(send_night transfer_two_destinations --source-seed "$SENDER_SEED" --unshielded-amount 5 \
        --destination-address "$RECEIVER_ADDRESS" --destination-address "$SPARE_ADDRESS") && pass "landed" || fail "$problem"
}

# --- Pre-fork contracts can be read and called, and their authorities still rotate ----------
check_store_contract_still_works() {  # <label> <funding seed>
    local problem
    [ -n "$(registry_address "$1")" ] || { skip "no contract $1 (group 2)"; return; }
    # The first check proves the value stored before the fork is still there.
    problem=$(store_contract_call "$1" "$2" check store check) || { fail "$problem"; return; }
    pass "the value stored before the fork checks out; a new value stored and checked"
}

check_contract_authority_rotated() {
    local problem
    [ -n "$(registry_address store-b)" ] || { skip "no contract store-b (group 2)"; return; }
    problem=$(store_contract_rotate_authority store-b "$RECEIVER_SEED" "$RECEIVER_SEED" "$SENDER_SEED" "$RECEIVER_SEED") \
        || { fail "$problem"; return; }
    pass "the receiver, a member since the rotation before the fork, signed; the committee is now sender and receiver"
}

check_micro_dao_state_readable() {
    local address entry_points
    address=$(registry_address micro-dao)
    [ -n "$address" ] || { skip "micro-dao was not deployed (group 2)"; return; }
    semantic_snapshot "$address" "$EV/micro_dao_state.txt" || { fail "micro-dao's state is unreadable"; return; }
    entry_points=$(grep -c '^op=' "$EV/micro_dao_state.txt")
    if diff -q "$(recorded_contract_state "$address")" "$EV/micro_dao_state.txt" > /dev/null; then
        pass "micro-dao's $entry_points entry points and authority read as recorded before the fork"
    else fail "micro-dao's entry points or authority differ from the record ($EV/micro_dao_state.txt)"; fi
}

# --- Pre-fork dApps, with their original artefacts and with recompiled ones ------------------
# Which of the two a tool needs differs per tool. Through the toolkit the original artefacts
# may be refused: KNOWN only with the error the fork lists; recompiled ones must work, on the
# address and with the private state from before the fork.
check_dapp_with_both_artefacts() {  # <dApp> <circuit> [argument]
    local name=$1 circuit=$2 argument="${3:-}" address original_worked=0 keys exit_code=0 log
    address=$(registry_address "$name")
    [ -n "$address" ] || { skip "$name was not deployed (group 2)"; return; }

    if DAPP_OUT=out dapp_call "$name" "$SENDER_SEED" "$address" "$circuit" ${argument:+"$argument"}; then original_worked=1; fi
    for log in "$(dapp_dir "$name")/${DAPP_FILE_PREFIX}_${circuit}"_*.log; do
        [ ! -e "$log" ] || [[ "$log" == *.original.log ]] || mv "$log" "${log%.log}.original.log"
    done
    if [ "$original_worked" = 0 ] && ! cat "$(dapp_dir "$name")/${DAPP_FILE_PREFIX}_${circuit}"_*.original.log 2> /dev/null | grep -qE "${KNOWN_OLD_ARTEFACTS_ERROR:-no known error}"; then
        fail "the original artefacts failed, and not with the known error: $(tail -n 2 "$(dapp_dir "$name")/${DAPP_FILE_PREFIX}_${circuit}"_*.original.log 2> /dev/null | tr '\n' ' ' | cut -c1-200)"
        return
    fi

    keys=$(dapp_recompile "$name" "$TK_IMAGE" "$SENDER_COIN_PUBLIC") || exit_code=$?
    [ "$exit_code" != 2 ] || { fail "recompiling with compactc $NEW_EXPECTED_COMPACTC failed: $keys"; return; }
    # bboard's take_down can only run once; if the original artefacts ran it, post again.
    if [ "$name" = bboard ] && [ "$original_worked" = 1 ]; then circuit=post; argument="posted-after-the-fork"; fi
    DAPP_OUT=out-new dapp_call "$name" "$SENDER_SEED" "$address" "$circuit" ${argument:+"$argument"} \
        || { fail "$circuit with the recompiled artefacts: $(dapp_err "$name" "$circuit")"; return; }

    keys=$(tr '\n' ';' <<< "$keys")
    if [ "$original_worked" = 1 ]; then pass "the original and the recompiled artefacts both work; $keys"
    else known "toolkit-js refuses the original ledger-$OLD_LEDGER artefacts ($KNOWN_OLD_ARTEFACTS_ISSUE); the recompiled ones work on the pre-fork address; $keys"; fi
}

# Midnight.js keeps a runtime for the old ledger, so it calls a pre-fork contract with the
# artefacts it was called with before the fork, and its private state carries on.
check_midnightjs_calls_counter_original_artefacts() {
    local counter count out="$EV/midnightjs_counter"
    counter=$(registry_address counter)
    [ -n "$counter" ] || { skip "no counter was deployed (group 2)"; return; }
    [ -d "$(dapp_dir counter)/out-mjs/contract" ] || { skip "Midnight.js did not call the counter before the fork (G2-midnightjs-calls-counter)"; return; }
    MN_ZK_VERIFY=warn client_run "$out" mjs_check.mjs retained-call --contract-dir "$(dapp_dir counter)/out-mjs" \
        --name counter --address "$counter" --circuit increment --times 3 || { fail "$(client_error "$out")"; return; }
    count=$(jq -r '.privateState.count // empty' "$out.json")
    if [ -z "${MIDNIGHTJS_COUNT_BEFORE_FORK:-}" ]; then
        pass "three increments on $counter with artefacts from before the fork; private count $count"
    elif [ "$count" = $(( MIDNIGHTJS_COUNT_BEFORE_FORK + 3 )) ]; then
        pass "three increments on $counter with artefacts from before the fork; its private count carries on: $MIDNIGHTJS_COUNT_BEFORE_FORK -> $count"
    else
        fail "private count $count, expected $(( MIDNIGHTJS_COUNT_BEFORE_FORK + 3 )): $MIDNIGHTJS_COUNT_BEFORE_FORK before the fork, then three more"
    fi
}

# A circuit the new compiler compiles differently gets a new verifier key, and the contract
# rejects its proofs until its maintenance authority, the deploying wallet, replaces the key.
check_changed_verifier_keys_replaced() {
    local dapp game address changed keys exit_code report="" addresses
    for dapp in rps vault; do
        exit_code=0
        keys=$(dapp_recompile "$dapp" "$TK_IMAGE" "$SENDER_COIN_PUBLIC") || exit_code=$?
        [ "$exit_code" != 2 ] || { fail "recompiling $dapp with compactc $NEW_EXPECTED_COMPACTC failed: $keys"; return; }
        changed=$(dapp_changed_keys "$dapp" | xargs -n1 basename 2> /dev/null | sed 's/\.verifier$//' | xargs)
        [ -n "$changed" ] || continue
        if [ "$dapp" = rps ]; then addresses="$(game_address half-revealed) $(game_address waiting)"; else addresses=$(vault_address); fi
        for address in $addresses; do
            dapp_update_keys "$dapp" "$SENDER_SEED" "$address" "$EV/replace_keys_${address:0:12}.log" \
                || { fail "$(registry_label "$address"): $(last_line_of "$EV/replace_keys_${address:0:12}.log")"; return; }
            report="$report $(registry_label "$address") ($changed);"
        done
    done
    if [ -z "$report" ]; then pass "compactc $NEW_EXPECTED_COMPACTC compiles every circuit of the games and the vault to the same verifier key: nothing to replace"
    else pass "the sender, as maintenance authority, replaced the verifier keys the new compiler changed:$report"; fi
}

# --- Tokens minted before the fork -------------------------------------------------------------
check_vault_token_pays_out() {
    local token before after problem
    token=$(vault_token)
    [ -n "$(vault_address)" ] && [ -n "$token" ] || { skip "no vault with a minted token (group 2)"; return; }
    before=$(token_held_by "$SENDER_SEED" "$token" vault_sender_before) || { fail "$before"; return; }
    problem=$(vault_call pay_out "$(wallet_address "$SENDER_SEED" user-address)" 2000) || { fail "$problem"; return; }
    problem=$(vault_call mint 500) || { fail "$problem"; return; }
    problem=$(vault_must_hold 997500) || { fail "$problem"; return; }
    after=$(token_held_by "$SENDER_SEED" "$token" vault_sender_after) || { fail "$after"; return; }
    gained_exactly "$before" "$after" 2000 || { fail "the sender holds $before -> $after of the token, expected 2000 more"; return; }
    pass "the vault paid out 2000 of the token it minted before the fork and minted 500 more: it holds 997500, the sender $after"
}

check_sdk_spends_minted_token() {
    local token held out="$EV/sdk_minted_token"
    token=$(vault_token)
    [ -n "$token" ] || { skip "no vault token (group 2)"; return; }
    client_run "${out}_sync" wallet_check.mjs sync || { fail "$(client_error "${out}_sync")"; return; }
    held=$(jq -r --arg token "$token" '.wallet.unshielded.balances[$token] // "0"' "${out}_sync.json")
    big_gt "$held" 0 || { fail "the wallet SDK shows none of the vault's token in the sender's balances"; return; }
    if client_run "${out}_send" wallet_check.mjs send --kind unshielded --amount 10 --token "$token"; then
        pass "the wallet SDK shows $held of the vault's token and sent 10 of it"
    else fail "the wallet SDK shows $held of the token, but the transfer failed: $(client_error "${out}_send")"; fi
}

# --- dApps left mid-flow before the fork ---------------------------------------------------------
# The guest committed to paper before the fork. Revealing scissors must be refused.
check_game_wrong_reveal_refused() {
    local honest_state cheating_state
    [ -n "$(game_address half-revealed)" ] || { skip "no half-revealed game (group 2)"; return; }
    honest_state=$(player_private_state_file half-revealed guest); cheating_state=$(player_private_state_file half-revealed cheat)
    [ -s "$honest_state" ] || { skip "the guest's private state is gone"; return; }
    jq ".move = $SCISSORS" "$honest_state" > "$cheating_state"
    if DAPP_PS=half-revealed_cheat dapp_call rps "$RECEIVER_SEED" "$(game_address half-revealed)" reveal; then
        fail "a reveal of scissors against a commitment to paper landed"
    elif grep -q "does not match the commitment" "$(dapp_log rps reveal_intent)"; then
        pass "refused by the circuit before anything was proved: the move does not match the commitment"
    else
        fail "refused, but not by the commitment check: $(dapp_err rps reveal)"
    fi
}

check_game_half_revealed_finished() {
    local address guest_before guest_after problem winnings=$(( GAME_STAKE * 2 ))
    address=$(game_address half-revealed)
    [ -n "$address" ] || { skip "no half-revealed game (group 2)"; return; }
    guest_before=$(unshielded_night_of "$RECEIVER_SEED" half_revealed_guest_before) || { fail "$guest_before"; return; }
    problem=$(game_call half-revealed guest reveal) || { fail "the guest's reveal of its pre-fork commitment: $problem"; return; }
    problem=$(game_call half-revealed guest settle) || { fail "$problem"; return; }
    guest_after=$(unshielded_night_of "$RECEIVER_SEED" half_revealed_guest_after) || { fail "$guest_after"; return; }
    gained_exactly "$guest_before" "$guest_after" "$winnings" || { fail "the guest's NIGHT went $guest_before -> $guest_after, expected $winnings more"; return; }
    problem=$(contract_must_hold_night "$address" 0) || { fail "$problem"; return; }
    pass "the guest opened the commitment it made on ledger $OLD_LEDGER; paper beats rock; the guest won both stakes ($winnings), the contract holds nothing"
}

check_game_waiting_cancelled() {
    local address host_before host_after problem
    address=$(game_address waiting)
    [ -n "$address" ] || { skip "no waiting game (group 2)"; return; }
    host_before=$(unshielded_night_of "$SENDER_SEED" waiting_host_before) || { fail "$host_before"; return; }
    problem=$(game_call waiting host cancel) || { fail "$problem"; return; }
    host_after=$(unshielded_night_of "$SENDER_SEED" waiting_host_after) || { fail "$host_after"; return; }
    gained_exactly "$host_before" "$host_after" "$GAME_STAKE" || { fail "the host's NIGHT went $host_before -> $host_after, expected $GAME_STAKE more"; return; }
    problem=$(contract_must_hold_night "$address" 0) || { fail "$problem"; return; }
    pass "the host, proving the key it committed with before the fork, cancelled and got its $GAME_STAKE back; the contract holds nothing"
}

check_game_draw_stays_settled() {
    local address problem
    address=$(game_address draw)
    [ -n "$address" ] || { skip "no settled game (group 2)"; return; }
    if game_call draw host reveal > /dev/null; then fail "a reveal on the game settled before the fork landed"; return; fi
    grep -q "both players must have joined" "$(dapp_log rps reveal_intent)" || { fail "refused, but not by the game's phase check: $(dapp_err rps reveal)"; return; }
    problem=$(contract_must_hold_night "$address" 0) || { fail "$problem"; return; }
    pass "still settled: a reveal is refused by the game's phase check; the contract holds nothing"
}

# --- The group, in runbook order --------------------------------------------------------------
bullet "Pre-fork balances can be spent, in every transfer type"
check transfer-unshielded           "an unshielded NIGHT transfer from pre-fork funds"                     check_transfer_unshielded
check transfer-shielded             "a shielded NIGHT transfer from pre-fork funds"                        check_transfer_shielded
check transfer-mixed                "one transaction with a shielded and an unshielded output"             check_transfer_mixed
check transfer-two-destinations     "one unshielded transfer to two destinations"                          check_transfer_two_destinations

bullet "Pre-fork contracts can be read and called, and their authorities still rotate"
check contract-store-a              "the one-member contract: its stored value checks out, store, check"   check_store_contract_still_works store-a "$SENDER_SEED"
check contract-store-b              "the two-member contract: its stored value checks out, store, check"   check_store_contract_still_works store-b "$RECEIVER_SEED"
check contract-authority-rotated    "the committee rotated before the fork rotates again"                  check_contract_authority_rotated
check micro-dao-state-readable      "micro-dao's entry points and authority read as recorded"              check_micro_dao_state_readable

bullet "Pre-fork dApps can be called, with their original artefacts and with recompiled ones"
check dapp-counter                  "counter.increment on the pre-fork counter, with both sets of artefacts" check_dapp_with_both_artefacts counter increment
check dapp-bboard                   "bboard.take_down with the secret from before the fork, with both sets of artefacts" check_dapp_with_both_artefacts bboard take_down
check midnightjs-calls-counter-original-artefacts "Midnight.js calls the pre-fork counter with its original artefacts" check_midnightjs_calls_counter_original_artefacts
check changed-verifier-keys-replaced "verifier keys the new compiler changed are replaced on the games and the vault" check_changed_verifier_keys_replaced

bullet "Tokens minted before the fork can be spent after it"
check vault-token-pays-out          "the vault pays out and mints the token it created before the fork"    check_vault_token_pays_out
check sdk-spends-minted-token       "the wallet SDK shows that token and spends it"                        check_sdk_spends_minted_token

bullet "dApps left mid-flow before the fork can be finished, and the funds they hold come out right"
check game-wrong-reveal-refused     "a reveal that does not match the guest's pre-fork commitment is refused" check_game_wrong_reveal_refused
check game-half-revealed-finished   "the half-revealed game is finished: the guest reveals, the winner is paid" check_game_half_revealed_finished
check game-waiting-cancelled        "the game nobody joined is cancelled and the host refunded"            check_game_waiting_cancelled
check game-draw-stays-settled       "the game settled before the fork stays settled"                       check_game_draw_stays_settled

group_finish
