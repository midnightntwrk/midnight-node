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

# Group 9: indexer, wallets and SDKs across the fork.
#
# Do the services and the client libraries follow the chain through the fork block and read
# both sides of it? Everything here looks at data from before the fork with the tools of
# after it.
set -e
source "$(dirname "${BASH_SOURCE[0]}")/../lib/suite.sh"

start_after_fork g9
require_clients
export DAPP_FILE_PREFIX=prefork
CROSSER_LOG="$EVIDENCE_DIR/g4_crosser/crosser.json"
SDK_WALLET_STATE="$STATE_DIR/sdk_wallet_state_before_fork.json"

group_start G9 "Indexer, wallets and SDKs across the fork, on $NETWORK_NAME (fork block #$FORK_HEIGHT)"

# --- The indexer ---------------------------------------------------------------------------
check_indexer_crossed_fork() {
    local problem boundary_error
    if [ "$TARGET" = local ]; then
        docker inspect -f '{{.State.Running}}' chain-indexer 2> /dev/null | grep -q true || { fail "the chain-indexer container is not running"; return; }
        boundary_error=$(docker logs chain-indexer 2>&1 | grep -aiE 'ledger state root mismatch|zswap state root mismatch|translate ledger state' | tail -1)
        [ -z "$boundary_error" ] || { fail "the chain indexer logged an error at the fork block: ${boundary_error:0:200}"; return; }
    fi
    if problem=$(indexer_catch_up 180); then pass "the indexer is at #$(indexer_height), past fork block #$FORK_HEIGHT and at the finalized tip"
    else fail "$problem"; fi
}

check_indexer_serves_both_sides() {
    local address before_fork=0 after_fork=0 missing=""
    for address in $(registry_addresses old); do
        before_fork=$(( before_fork + 1 ))
        [ -n "$(indexer_contract_type "$address")" ] || missing="$missing $(registry_label "$address")"
    done
    for address in $(registry_addresses new); do
        after_fork=$(( after_fork + 1 ))
        [ -n "$(indexer_contract_type "$address")" ] || missing="$missing $(registry_label "$address")"
    done
    if [ -n "$missing" ]; then fail "not in the indexer:$missing"
    elif [ "$before_fork" = 0 ] || [ "$after_fork" = 0 ]; then fail "$before_fork contracts from before the fork and $after_fork from after it: both sides are needed"
    else pass "$before_fork contracts deployed before the fork and $after_fork deployed after it are all served"; fi
}

# Everything the groups after the fork sent, looked up by hash.
check_node_and_indexer_agree_on_transactions() {
    local report
    if report=$(tx_inclusion_check "$EV/tx_inclusion.tsv" "$EVIDENCE_DIR"/g[5-8]/*.log "$DAPPS_DIR"/*/*send.log); then pass "$report"
    else fail "$report ($EV/tx_inclusion.tsv)"; fi
}

# --- Wallets ---------------------------------------------------------------------------------
# The sender's wallet as the wallet SDK sees it now: what the next two checks must reach.
SENDER_WALLET_NOW="$EV/sdk_sync.json"
# Once per run of this script: a file left by an earlier run shows older balances.
sync_sender_wallet() {
    [ -n "${SENDER_WALLET_SYNCED:-}" ] && return
    client_run "$EV/sdk_sync" wallet_check.mjs sync && SENDER_WALLET_SYNCED=1
}

check_wallet_restores_from_genesis() {
    local out="$EV/sdk_restore_from_genesis"
    sync_sender_wallet || { fail "the sender's wallet does not sync: $(client_error "$EV/sdk_sync")"; return; }
    if client_run "$out" wallet_check.mjs restore --expect-json "$SENDER_WALLET_NOW"; then
        pass "a wallet restored from its seed synced from genesis across the fork in $(jq -r .restoredIn "$out.json") ms and shows the same balances"
    else fail "$(client_error "$out")"; fi
}

check_wallet_resumes_from_saved_state() {
    local out="$EV/sdk_resume_from_saved_state"
    [ -s "$SDK_WALLET_STATE" ] || { skip "no wallet state was saved before the fork (G2-sdk-wallet-state-saved)"; return; }
    sync_sender_wallet || { fail "the sender's wallet does not sync: $(client_error "$EV/sdk_sync")"; return; }
    if client_run "$out" wallet_check.mjs resume --from "$SDK_WALLET_STATE" --expect-json "$SENDER_WALLET_NOW"; then
        pass "the state saved on ledger $OLD_LEDGER restored and synced on across the fork in $(jq -r .resumedIn "$out.json") ms; it shows the same balances"
    else fail "$(client_error "$out")"; fi
}

# The receiver's address was printed before the fork. A user who was given it then pays to it now.
check_transfer_to_pre_fork_address() {
    local before after out="$EV/sdk_transfer_to_pre_fork_address"
    CLIENT_SEED="$RECEIVER_SEED" client_run "${out}_before" wallet_check.mjs sync || { fail "the receiver's wallet: $(client_error "${out}_before")"; return; }
    client_run "$out" wallet_check.mjs send --kind unshielded --amount 7 --to "$RECEIVER_ADDRESS" || { fail "$(client_error "$out")"; return; }
    wait_blocks 4 > /dev/null || true
    CLIENT_SEED="$RECEIVER_SEED" client_run "${out}_after" wallet_check.mjs sync || { fail "the receiver's wallet afterwards: $(client_error "${out}_after")"; return; }
    before=$(jq -r --arg night "$NIGHT_TOKEN_TYPE" '.wallet.unshielded.balances[$night] // "0"' "${out}_before.json")
    after=$(jq -r --arg night "$NIGHT_TOKEN_TYPE" '.wallet.unshielded.balances[$night] // "0"' "${out}_after.json")
    if gained_exactly "$before" "$after" 7; then pass "7 NIGHT sent through the wallet SDK to ${RECEIVER_ADDRESS:0:24}..., printed before the fork; the receiver's wallet went $before -> $after"
    else fail "the receiver's wallet went $before -> $after, expected 7 more"; fi
}

check_history_covers_both_sides() {
    local history old_entries new_entries
    sync_sender_wallet || { fail "the sender's wallet does not sync: $(client_error "$EV/sdk_sync")"; return; }
    history=$(jq -c '.history // empty' "$SENDER_WALLET_NOW")
    [ -n "$history" ] || { fail "the wallet's sync output has no history"; return; }
    old_entries=$(jq -r ".byEra.v$OLD_LEDGER // 0" <<< "$history"); new_entries=$(jq -r ".byEra.v$NEW_LEDGER // 0" <<< "$history")
    if [ "$old_entries" -gt 0 ] && [ "$new_entries" -gt 0 ]; then pass "the sender's history has $old_entries ledger-$OLD_LEDGER and $new_entries ledger-$NEW_LEDGER entries"
    else fail "expected entries from both ledgers: $history"; fi
}

# --- Midnight.js ---------------------------------------------------------------------------------
check_midnightjs_reads_pre_fork_contracts() {
    local address label out total=0 decoded=0 undecoded="" stale=""
    for address in $(registry_addresses old); do
        total=$(( total + 1 )); label=$(registry_label "$address"); out="$EV/midnightjs_state_${address:0:8}"
        if client_run "$out" mjs_check.mjs decode-state --address "$address"; then decoded=$(( decoded + 1 ))
        elif ! contract_state_node_vs_indexer "$address" "$out" > /dev/null && known_indexer_stale_state "$address" "$out" "$FORK_HEIGHT"; then stale="$stale $label"
        else undecoded="$undecoded $label: $(client_error "$out" | cut -c1-90);"; fi
    done
    if [ "$total" = 0 ]; then skip "no contract was deployed before the fork"
    elif [ -n "$undecoded" ]; then fail "$decoded of $total decoded;$undecoded"
    elif [ -n "$stale" ]; then known "$decoded of $total decoded; the indexer serves the last pre-fork state, data unchanged, for$stale ($KNOWN_INDEXER_STALE_STATE_ISSUE)"
    else pass "Midnight.js decodes the state of all $total contracts deployed before the fork"; fi
}

check_midnightjs_reads_pre_fork_transaction() {
    local out="$EV/midnightjs_pre_fork_transaction" era block
    [ -n "${MIDNIGHTJS_TRANSACTION_BEFORE_FORK:-}" ] || { skip "Midnight.js sent no transaction before the fork (G2-midnightjs-calls-counter)"; return; }
    client_run "$out" mjs_check.mjs tx-data --tx-id "$MIDNIGHTJS_TRANSACTION_BEFORE_FORK" || { fail "$(client_error "$out")"; return; }
    era=$(jq -r .version "$out.json"); block=$(jq -r .blockHeight "$out.json")
    if [ "$era" = "v$OLD_LEDGER" ] && [ "$block" -lt "$FORK_HEIGHT" ] && [ "$(jq -r .decoded "$out.json")" = true ]; then
        pass "the transaction Midnight.js sent before the fork is found at block #$block and decoded as a ledger-$OLD_LEDGER record"
    else
        fail "found as era $era at block #$block, decoded: $(jq -r .decoded "$out.json") (fork block #$FORK_HEIGHT)"
    fi
}

# --- The SDK across the fork block itself ------------------------------------------------------
# The crosser's wallet process ran through the fork (group 4); its log shows the switch.
check_sdk_switched_at_fork_block() {
    local fork_timestamp verdict
    [ -s "$CROSSER_LOG" ] || { skip "no wallet ran across the fork (G4-crosser-ran-across-fork)"; return; }
    fork_timestamp=$(gql_query "{ block(offset: { height: $FORK_HEIGHT }) { timestamp } }" | jq -r '.data.block.timestamp // empty')
    [ -n "$fork_timestamp" ] || { fail "the indexer has no timestamp for fork block #$FORK_HEIGHT"; return; }
    verdict=$(python3 "$SUITE_DIR/lib/soak.py" "$CROSSER_LOG" "$FORK_HEIGHT" "$fork_timestamp" "$NEW_SPEC_FLOOR" "$OLD_LEDGER" "$NEW_LEDGER" crossed)
    case "$verdict" in
        PASS*) pass "the crosser's wallet, without a restart: ${verdict#PASS }" ;;
        WARN*) warn "${verdict#WARN }" ;;
        *)     fail "${verdict#FAIL }" ;;
    esac
}

# --- The toolkit ---------------------------------------------------------------------------------
check_toolkit_fetches_every_block() {
    if fetch_every_block "$EV/fetch.log"; then pass "every block from genesis to #$(node_height) fetched and decoded, across fork block #$FORK_HEIGHT"
    else fail "$(last_line_of "$EV/fetch.log")"; fi
}

check_faucet_answers() {
    [ "$TARGET" = network ] || { skip "local-env has no faucet"; return; }
    [ -n "${FAUCET_URL:-}" ] || { skip "no FAUCET_URL in the network settings"; return; }
    local status; status=$(curl -s -m 15 -o /dev/null -w '%{http_code}' "$FAUCET_URL")
    if [ "$status" = 200 ]; then pass "$FAUCET_URL answers 200 (a probe: no payout is requested)"
    else warn "$FAUCET_URL answers $status"; fi
}

# --- The group, in runbook order -------------------------------------------------------------
bullet "The indexer follows the chain through the fork block and serves blocks from both sides"
check indexer-crossed-fork                    "the indexer crossed the fork block and follows the finalized tip"        check_indexer_crossed_fork
check indexer-serves-both-sides               "contracts deployed before and after the fork are all served"             check_indexer_serves_both_sides

bullet "The node and the indexer agree on contract state and transactions on both sides"
check node-and-indexer-agree-on-state         "node and indexer serve the same state for every contract"                check_node_and_indexer_agree_on_state
check node-and-indexer-agree-on-transactions  "every transaction sent after the fork is in a block, found by hash"      check_node_and_indexer_agree_on_transactions

bullet "A wallet synced before the fork carries on after it; a saved wallet state restores"
check wallet-restores-from-genesis            "a wallet restored from its seed syncs from genesis across the fork"      check_wallet_restores_from_genesis
check wallet-resumes-from-saved-state         "the wallet state saved before the fork resumes after it"                 check_wallet_resumes_from_saved_state
check transfer-to-pre-fork-address            "a transfer to an address printed before the fork arrives"                check_transfer_to_pre_fork_address

bullet "Transaction history covers both sides of the fork"
check history-covers-both-sides               "the wallet SDK's history has entries from both ledgers"                  check_history_covers_both_sides

bullet "Midnight.js reads pre-fork contracts and transactions"
check midnightjs-reads-pre-fork-contracts     "Midnight.js decodes the state of every contract deployed before the fork" check_midnightjs_reads_pre_fork_contracts
check midnightjs-reads-pre-fork-transaction   "Midnight.js finds a transaction from before the fork and decodes it"     check_midnightjs_reads_pre_fork_transaction

bullet "The SDK knows the fork schedule and switches at the fork block"
check sdk-switched-at-fork-block              "a running wallet switched from ledger $OLD_LEDGER to ledger $NEW_LEDGER at the fork block" check_sdk_switched_at_fork_block

bullet "The toolkit can fetch and decode every block from genesis to the tip"
check toolkit-fetches-every-block             "the new toolkit fetches and decodes every block on both sides of the fork" check_toolkit_fetches_every_block

bullet "The Cardano side carries on"
not_covered cardano-side                      "cNIGHT observation, governance sync and committee rotation after the fork" "only the bridge pallet has a check (group 7)"

bullet "The faucet is updated and pays out on the new ledger"
check faucet-answers                          "the faucet answers"                                                      check_faucet_answers

group_finish
