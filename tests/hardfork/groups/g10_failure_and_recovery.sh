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

# Group 10: failure and recovery drills.
#
# Does the network survive nodes dying, lagging and being wiped, and does the safety net work?
#
# This script has the drills that need no operator: a node killed without warning, and the
# safety net (safe mode). The others break nodes on purpose and belong to whoever
# operates them:
#   validators dropping out, a laggard    local/rollout.sh; network/dropout.sh
#   a wiped validator, a wiped indexer    local/wipe_validator.sh, local/wipe_indexer.sh
# They record into this group's table too.
#
# On a shared network run this group last. The safe-mode drill refuses every user
# transaction while it lasts, so on a network it runs only with SAFE_MODE_DRILL=1.
# After the drill the toolkit cannot replay the chain any more (see the drill), so nothing
# that needs the toolkit may run after this script.
set -e
source "$(dirname "${BASH_SOURCE[0]}")/../lib/suite.sh"

start_after_fork g10
require_clients
SAFE_MODE_BLOCKS=$(( 7 * 14400 ))   # governance enters safe mode for 7 days of 6-second blocks

group_start G10 "Failure and recovery drills on $NETWORK_NAME (node $(node_version), spec $(spec_version))"

# --- Unclean shutdown -------------------------------------------------------------------------
check_node_survives_sigkill() {
    [ "$TARGET" = local ] || { skip "killing a node is the operators' call on a network"; return; }
    local victim=midnight-node-4 url best tip finalized block_hash attempt
    url=$(node_url "$victim")
    docker kill -s KILL "$victim" > /dev/null 2>&1 || true
    sleep 3
    docker start "$victim" > /dev/null
    for attempt in $(seq 1 30); do
        best=$(best_height_at "$url"); tip=$(node_height)
        [ -n "$best" ] && [ -n "$tip" ] && [ "$best" -ge $(( tip - 2 )) ] && break
        sleep 10
    done
    if docker logs "$victim" --since 6m 2>&1 | grep -qE 'root should be in the arena|IncompatibleColumnConfig|panicked'; then
        fail "$victim logged a database error or a panic after the restart"
        return
    fi
    [ -n "$best" ] && [ "$best" -ge $(( tip - 2 )) ] || { fail "$victim is at #${best:-unreachable}, the tip is #$tip"; return; }
    finalized=$(finalized_height); block_hash=$(block_hash_at "$NODE_HTTP" "$finalized")
    if [ "$(state_root_at "$url" "$block_hash")" = "$(state_root_at "$NODE_HTTP" "$block_hash")" ]; then
        pass "$victim was killed with SIGKILL, restarted from its own database, caught up to #$best and agrees on the state at finalized #$finalized"
    else fail "$victim caught up, but its state root at finalized #$finalized differs"; fi
}

# --- The safety net: safe mode ------------------------------------------------------------------
# A migration that fails must put the chain in safe mode, not freeze it. Triggering that
# needs a broken runtime and is left to the runtime's own tests. Here: the pallet is there,
# the fork did not trigger it, and governance can enter and leave it.

# Events of a block range, by pallet. A block whose events cannot be decoded counts as
# unread, never as a block without events.
read_events() {  # <from> <to> <pallets> <out.json>; prints the reason when not every block was read
    local undecoded
    chain_check events --from "$1" --to "$2" --sections "$3" > "$4" 2> "${4%.json}.err" \
        || { echo "reading the events of blocks #$1..#$2 failed: $(last_line_of "${4%.json}.err")"; return 1; }
    undecoded=$(jq -r '.decodeErrors.count // "unknown"' "$4" 2> /dev/null)
    [ "$undecoded" = 0 ] || { echo "blocks #$1..#$2: the events of some blocks could not be decoded ($(jq -r '.decodeErrors.blocks // [] | join(", ")' "$4" 2> /dev/null))"; return 1; }
}
event_count() { jq -r --arg event "$2" '.counts[$event] // 0' "$1"; }   # <events.json> <pallet.Event>

check_safe_mode_present_and_idle() {
    chain_check safe-mode > "$EV/safe_mode.json" 2> "$EV/safe_mode.err" || { fail "$(last_line_of "$EV/safe_mode.err")"; return; }
    if [ "$(jq -r .palletPresent "$EV/safe_mode.json")" != true ]; then fail "the runtime has no SafeMode pallet"
    elif [ "$(jq -r .isEntered "$EV/safe_mode.json")" != false ]; then fail "safe mode is entered, until block $(jq -r .enteredUntil "$EV/safe_mode.json")"
    else pass "the SafeMode pallet is in the runtime and safe mode is not entered"; fi
}

check_safe_mode_not_triggered_by_fork() {
    local first=$(( FORK_HEIGHT - 1 )) last=$(( FORK_HEIGHT + 40 )) events="$EV/events_around_fork.json" problem summary
    problem=$(read_events "$first" "$last" safeMode,multiBlockMigrations "$events") || { fail "$problem"; return; }
    summary="migration started: $(event_count "$events" multiBlockMigrations.UpgradeStarted), completed: $(event_count "$events" multiBlockMigrations.UpgradeCompleted), failed: $(event_count "$events" multiBlockMigrations.MigrationFailed); safe mode entered: $(event_count "$events" safeMode.Entered)"
    if [ "$(event_count "$events" multiBlockMigrations.MigrationFailed)" != 0 ] || [ "$(event_count "$events" safeMode.Entered)" != 0 ]; then
        fail "blocks #$first..#$last: $summary"
    elif [ "$(event_count "$events" multiBlockMigrations.UpgradeStarted)" = 0 ] || [ "$(event_count "$events" multiBlockMigrations.UpgradeCompleted)" = 0 ]; then
        fail "the migration did not start and complete within blocks #$first..#$last: $summary"
    else
        pass "blocks #$first..#$last: $summary"
    fi
}

# --- The drill ------------------------------------------------------------------------------------
governance_call() {  # <call> <event it must emit> <out prefix>
    chain_check governance-root-call --council-uris "$GOV_COUNCIL_URIS" --tc-uris "$GOV_TC_URIS" --executor-uri "$GOV_EXECUTOR_URI" \
        --call "$1" --expect-event "$2" > "$3.json" 2> "$3.err"
}
safe_mode_is_entered() { [ "$(chain_check safe-mode 2> /dev/null | jq -r .isEntered 2> /dev/null)" != false ]; }

# A stuck safe mode refuses every user transaction, so leaving it is tried twice.
leave_safe_mode() {
    governance_call safeMode.forceExit safeMode.Exited "$EV/force_exit" && return 0
    wait_blocks 2 > /dev/null || true
    safe_mode_is_entered || return 0
    governance_call safeMode.forceExit safeMode.Exited "$EV/force_exit_retry"
}

# One user transfer. After the drill's refused transaction the toolkit cannot replay the
# chain (it applies a transaction the node did not: a toolkit limitation, not filed), so the
# wallet SDK sends when the toolkit stops with that error.
send_user_transfer() {  # <evidence name> -> sent or not-sent
    local log="$EV/$1.log"
    if tk_tx generate-txs single-tx --source-seed "$SENDER_SEED" --unshielded-amount 1 --destination-address "$SPARE_ADDRESS" -d "$NODE_WS" > "$log" 2>&1; then
        echo sent; return
    fi
    grep -q StateRootMismatch "$log" || { echo not-sent; return; }
    proof_server_up new > /dev/null 2>&1 || true
    if client_run "$EV/$1_sdk" wallet_check.mjs send --kind unshielded --amount 1; then echo sent; else echo not-sent; fi
}

# A transaction refused by safe mode fails with frame_system's CallFiltered error.
CALL_FILTERED='select(.method? == "ExtrinsicFailed") | select((.data[0].module.error? // "") == "0x05000000")'

# What happened to user transactions in a block range: applied, refused or none.
user_transactions_in() {  # <from> <to>
    local system_events="$EV/system_events_$1_$2.json" midnight_events="$EV/midnight_events_$1_$2.json" refused applied
    read_events "$1" "$2" system "$system_events" > /dev/null && read_events "$1" "$2" midnight "$midnight_events" > /dev/null || { echo unreadable; return; }
    refused=$(jq "[.. | objects | $CALL_FILTERED] | length" "$system_events"); applied=$(jq '[.counts[]] | add // 0' "$midnight_events")
    if [ "$applied" != 0 ]; then echo applied
    elif [ "$refused" != 0 ]; then echo refused
    else echo none; fi
}

check_safe_mode_drill() {
    [ "$SAFE_MODE_DRILL" = 1 ] || { skip "the drill is off on this target (SAFE_MODE_DRILL=1 and the governance keys turn it on)"; return; }
    [ -n "${GOV_COUNCIL_URIS:-}" ] && [ -n "${GOV_TC_URIS:-}" ] && [ -n "${GOV_EXECUTOR_URI:-}" ] \
        || { fail "SAFE_MODE_DRILL=1 needs GOV_COUNCIL_URIS, GOV_TC_URIS and GOV_EXECUTOR_URI"; return; }
    local height_before entered_until blocks_ahead first last while_entered after_exit sent problems=""

    # 1. Governance enters safe mode.
    height_before=$(node_height)
    if ! governance_call safeMode.forceEnter safeMode.Entered "$EV/force_enter"; then
        if safe_mode_is_entered; then
            echo "!!! force_enter reported a failure, but safe mode may be entered: leaving it" >&2
            if leave_safe_mode && ! safe_mode_is_entered; then fail "force_enter reported a failure, yet the chain entered safe mode; force_exit restored it: $(last_line_of "$EV/force_enter.err")"
            else fail "force_enter reported a failure and safe mode is still entered: LEAVE IT BY HAND ($(last_line_of "$EV/force_enter.err"))"; fi
        else
            fail "force_enter through governance: $(last_line_of "$EV/force_enter.err")"
        fi
        return
    fi
    entered_until=$(jq -r '.events[] | select(.section == "safeMode" and .method == "Entered") | .data | if type == "array" then .[0] else (.until // .[0]) end' "$EV/force_enter.json" | head -1)
    blocks_ahead=$(( entered_until - $(node_height) ))
    [ "$blocks_ahead" -ge $(( SAFE_MODE_BLOCKS - 20 )) ] && [ "$blocks_ahead" -le $(( SAFE_MODE_BLOCKS + 20 )) ] \
        || problems="$problems safe mode lasts $blocks_ahead blocks, expected about $SAFE_MODE_BLOCKS;"

    # 2. The chain keeps producing blocks, and refuses a user transaction.
    wait_blocks 3 > /dev/null || problems="$problems block production stalled in safe mode;"
    first=$(node_height); send_user_transfer transfer_in_safe_mode > /dev/null; wait_blocks 3 > /dev/null || true; last=$(node_height)
    remember SAFE_MODE_DRILL_FROM "$first"
    while_entered=$(user_transactions_in "$first" "$last")
    case "$while_entered" in
        refused) ;;
        applied) problems="$problems a user transaction was applied in safe mode (blocks #$first..#$last);" ;;
        *)       problems="$problems no refused user transaction found in blocks #$first..#$last ($while_entered);" ;;
    esac

    # 3. Governance leaves safe mode.
    leave_safe_mode || problems="$problems force_exit failed twice;"
    ! safe_mode_is_entered || problems="$problems SAFE MODE IS STILL ENTERED: LEAVE IT BY HAND;"

    # 4. A user transaction lands again.
    first=$(node_height); sent=$(send_user_transfer transfer_after_safe_mode); wait_blocks 3 > /dev/null || true; last=$(node_height)
    after_exit=$(user_transactions_in "$first" "$last")

    if [ -n "$problems" ]; then
        fail "$problems"
    elif [ "$sent" = sent ] && [ "$after_exit" = applied ]; then
        pass "entered for $blocks_ahead blocks; blocks kept coming (#$height_before -> #$last); a user transaction was refused, included as a failed extrinsic; after force_exit a user transaction landed"
    elif [ "$sent" = not-sent ] && [ -n "${KNOWN_INDEXER_SHOWS_REFUSED_AS_APPLIED_ISSUE:-}" ] \
        && [ "$(client_custom_error "$EV/transfer_after_safe_mode_sdk")" = "$KNOWN_INDEXER_SHOWS_REFUSED_AS_APPLIED_ERROR" ]; then
        known "entered, a user transaction refused, left again; that a transaction lands afterwards could not be verified: the toolkit cannot replay past the refused extrinsic, and the wallet SDK's DUST view comes from an indexer that counted it as applied ($KNOWN_INDEXER_SHOWS_REFUSED_AS_APPLIED_ISSUE)"
    else
        fail "after force_exit no user transaction landed (sender: $sent; blocks #$first..#$last: $after_exit)"
    fi
}

# The indexer must not show a transaction that safe mode refused as applied.
check_indexer_ignores_refused_transaction() {
    local first="${SAFE_MODE_DRILL_FROM:-}" tip from to height answer problem blocks="" recorded="" as_applied=""
    [ -n "$first" ] || { skip "no drill ran (G10-safe-mode-drill)"; return; }
    tip=$(node_height); from=$first
    while [ "$from" -le "$tip" ]; do
        to=$(( from + 39 )); [ "$to" -le "$tip" ] || to=$tip
        problem=$(read_events "$from" "$to" system "$EV/system_events_scan.json") || { fail "$problem"; return; }
        blocks="$blocks $(jq -r ".blocks[]? | select([.events[]? | $CALL_FILTERED] | length > 0) | .height" "$EV/system_events_scan.json" | tr '\n' ' ')"
        from=$(( to + 1 ))
    done
    blocks=$(xargs <<< "$blocks")
    [ -n "$blocks" ] || { skip "no block with a refused transaction since #$first"; return; }
    indexer_catch_up 180 > /dev/null || true
    for height in $blocks; do
        answer=$(gql_query "{ block(offset: { height: $height }) { transactions { hash ... on RegularTransaction { transactionResult { status } } } } }")
        echo "$answer" > "$EV/indexer_block_$height.json"
        recorded="$recorded #$height: $(jq -r '[.data.block.transactions[]? | .transactionResult.status // "n/a"] | join(", ")' <<< "$answer");"
        jq -e '[.data.block.transactions[]? | select(.transactionResult.status == "SUCCESS")] | length > 0' <<< "$answer" > /dev/null 2>&1 && as_applied="$as_applied #$height"
    done
    if [ -z "$as_applied" ]; then pass "the indexer does not show the refused transaction as applied:$recorded"
    elif [ -n "${KNOWN_INDEXER_SHOWS_REFUSED_AS_APPLIED_ISSUE:-}" ]; then known "the indexer shows SUCCESS in block(s)$as_applied for a transaction the node refused ($KNOWN_INDEXER_SHOWS_REFUSED_AS_APPLIED_ISSUE):$recorded"
    else fail "the indexer shows SUCCESS in block(s)$as_applied for a transaction the node refused:$recorded"; fi
}

# --- The group ------------------------------------------------------------------------------------
bullet "Unclean shutdown: a node killed without warning recovers its database"
check node-survives-sigkill                  "a validator killed with SIGKILL restarts from its database and catches up"      check_node_survives_sigkill

bullet "Recovery from a snapshot"
not_covered recovery-from-snapshot           "a node restored from a snapshot taken before the fork, and one from after it"   "the suite takes and restores no snapshot"

bullet "The safety net is present, idle, and works in a drill"
check safe-mode-present-and-idle             "the SafeMode pallet is in the runtime and safe mode is not entered"            check_safe_mode_present_and_idle
check safe-mode-not-triggered-by-fork        "the migration started and completed at the fork without entering safe mode"    check_safe_mode_not_triggered_by_fork
check safe-mode-drill                        "governance enters safe mode, a user transaction is refused, governance leaves it, a user transaction lands" check_safe_mode_drill
check indexer-ignores-refused-transaction    "the indexer does not show the refused transaction as applied"                  check_indexer_ignores_refused_transaction

group_finish
