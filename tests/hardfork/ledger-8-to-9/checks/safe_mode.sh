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

# Table SAFE: SafeMode after the fork (node#2079). A failed multi-block migration enters safe
# mode instead of freezing the chain; triggering that needs a broken runtime and is left to the
# runtime tests. Here: the pallet, the fork window and a governance drill. The drill filters
# every user transaction while entered, so on a network it runs only with SAFE_MODE_DRILL=1.
set -e
source "$(dirname "${BASH_SOURCE[0]}")/../lib/suite.sh"

EV="$EVIDENCE_DIR/safe_mode"; mkdir -p "$EV"
use_toolkit l9
indexer_resolve || true
clients_ready > "$EV/clients_ready.log" 2>&1 || cannot_run "npm ci in clients/ failed"
state_load "$PRE_FORK_STATE"; state_load "$FORK_STATE"
[ -n "${ADDR_U4:-}" ] || ADDR_U4=$(tk_address "$SEED_4" --unshielded)
require_era l9
FORCE_DURATION=$(( 7 * 14400 ))   # 7 days of 6 s blocks

t_table SAFE "SafeMode (node#2079) on $NETWORK_NAME, node $(node_version), spec $(spec_version)"

c_present() {
    chain_check safe-mode > "$EV/safe_mode.json" 2>"$EV/safe_mode.err" || { t_fail "$(last_line_of "$EV/safe_mode.err")"; return; }
    if [ "$(jq -r .palletPresent "$EV/safe_mode.json")" != true ]; then t_fail "no SafeMode pallet in the runtime"
    elif [ "$(jq -r .isEntered "$EV/safe_mode.json")" != false ]; then t_fail "safe mode is entered until block $(jq -r .enteredUntil "$EV/safe_mode.json")"
    else t_pass "pallet index $(jq -r .palletIndex "$EV/safe_mode.json"), EnteredUntil = None"; fi
}
# A block whose events could not be decoded counts as unread, never as a block without events.
scan_events() {  # <from> <to> <sections> <out.json>: 1 with the reason on stdout when not every block was read
    chain_check events --from "$1" --to "$2" --sections "$3" > "$4" 2>"${4%.json}.err" \
        || { echo "event scan $1-$2 failed: $(last_line_of "${4%.json}.err")"; return 1; }
    local n; n=$(jq -r '.decodeErrors.count // "?"' "$4" 2>/dev/null)
    [ "$n" = 0 ] || { echo "blocks not decoded in $1-$2: $(jq -r '.decodeErrors.blocks // ["?"] | join(",")' "$4" 2>/dev/null)"; return 1; }
}
c_fork_window() {
    [ -n "${FORK_HEIGHT:-}" ] || { t_skip "fork height unknown (l9_preserved.sh records it)"; return; }
    local from=$(( FORK_HEIGHT - 1 )) to=$(( FORK_HEIGHT + 40 )) counts hits=0 n why
    why=$(scan_events "$from" "$to" safeMode,multiBlockMigrations "$EV/fork_events.json") || { t_fail "$why"; return; }
    cnt() { jq -r --arg k "$1" '.counts[$k] // 0' "$EV/fork_events.json"; }
    counts="UpgradeStarted=$(cnt multiBlockMigrations.UpgradeStarted) UpgradeCompleted=$(cnt multiBlockMigrations.UpgradeCompleted) MigrationFailed=$(cnt multiBlockMigrations.MigrationFailed) Entered=$(cnt safeMode.Entered)"
    if logs_available; then for n in $(log_nodes); do hits=$(( hits + $(node_logs "$n" | grep -ci 'entered safe mode' || true) )); done; fi
    if [ "$(cnt multiBlockMigrations.MigrationFailed)" != 0 ] || [ "$(cnt safeMode.Entered)" != 0 ]; then t_fail "blocks $from-$to: $counts"
    elif [ "$hits" != 0 ]; then t_fail "$hits 'entered safe mode' log lines"
    # Without the migration in the window, the window is wrong or the migration outlasted it.
    elif [ "$(cnt multiBlockMigrations.UpgradeStarted)" = 0 ] || [ "$(cnt multiBlockMigrations.UpgradeCompleted)" = 0 ]; then t_fail "the migration did not start and complete in blocks $from-$to: $counts"
    else t_pass "blocks $from-$to: $counts"; fi
}
t_check SAFE-1 "node#2079" "the SafeMode pallet is in the runtime and not entered" c_present
t_check SAFE-2 "HF-15, node#2079" "no MigrationFailed and no safeMode.Entered in the fork window" c_fork_window

gov() { chain_check governance-root-call --council-uris "$GOV_COUNCIL_URIS" --tc-uris "$GOV_TC_URIS" --executor-uri "$GOV_EXECUTOR_URI" "$@"; }
# The wallet SDK sends when the toolkit can no longer replay the chain (SAFE-4); the chain,
# not the sender, decides the verdict.
send_user_tx() {  # <log> -> prints toolkit-ok | toolkit-refused | sdk-ok | sdk-failed
    if tk_tx generate-txs single-tx --source-seed "$SEED_1" --unshielded-amount 1 --destination-address "$ADDR_U4" -d "$NODE_WS" > "$1" 2>&1; then echo toolkit-ok; return; fi
    grep -q StateRootMismatch "$1" || { echo toolkit-refused; return; }
    proof_server_up l9 >/dev/null 2>&1 || true
    if CLIENT_SEED="$SEED_1" client_run "$1.sdk" wallet_check.mjs send --kind unshielded --amount 1; then echo sdk-ok; else echo sdk-failed; fi
}
CALL_FILTERED='select(.method? == "ExtrinsicFailed") | select((.data[0].module.error? // "") == "0x05000000")'   # frame_system error 5
# Both print ERR when not every block in the range was read.
call_filtered_between() {
    scan_events "$1" "$2" system "$EV/system_$1_$2.json" >/dev/null && jq "[.. | objects | $CALL_FILTERED] | length" "$EV/system_$1_$2.json" || echo ERR
}
midnight_events_between() {
    scan_events "$1" "$2" midnight "$EV/midnight_$1_$2.json" >/dev/null && jq '[.counts[]] | add // 0' "$EV/midnight_$1_$2.json" || echo ERR
}
safe_mode_entered() { chain_check safe-mode 2>/dev/null | jq -r .isEntered 2>/dev/null; }
force_exit() {  # <out-prefix>: one retry, a stuck safe mode filters every user transaction
    gov --call safeMode.forceExit --expect-event safeMode.Exited > "$1.json" 2>"$1.err" && return 0
    wait_blocks 2
    [ "$(safe_mode_entered)" = false ] && return 0
    gov --call safeMode.forceExit --expect-event safeMode.Exited > "$1.retry.json" 2>"$1.retry.err"
}

c_drill() {
    if [ "$SAFE_MODE_DRILL" != 1 ]; then t_skip "drill off (SAFE_MODE_DRILL=1 and the governance key URIs turn it on)"; return; fi
    [ -n "${GOV_COUNCIL_URIS:-}" ] && [ -n "${GOV_TC_URIS:-}" ] && [ -n "${GOV_EXECUTOR_URI:-}" ] || { t_fail "SAFE_MODE_DRILL=1 needs GOV_COUNCIL_URIS, GOV_TC_URIS, GOV_EXECUTOR_URI"; return; }
    local h0 entered_until delta h1 hs he sent cf ms problems="" notes=""
    h0=$(node_height)
    if ! gov --call safeMode.forceEnter --expect-event safeMode.Entered > "$EV/force_enter.json" 2>"$EV/force_enter.err"; then
        local why; why=$(last_line_of "$EV/force_enter.err")
        if [ "$(safe_mode_entered)" = false ]; then t_fail "force_enter through governance: $why"; return; fi
        echo "!!! force_enter failed but safe mode may be entered: trying force_exit" >&2
        force_exit "$EV/force_exit_after_failed_enter" && [ "$(safe_mode_entered)" = false ] \
            && t_fail "force_enter reported a failure ($why) yet the chain entered safe mode; force_exit restored it" \
            || t_fail "force_enter reported a failure ($why) and safe mode is still entered or unknown: EXIT IT BY HAND"
        return
    fi
    entered_until=$(jq -r '.events[] | select(.section == "safeMode" and .method == "Entered") | .data | if type == "array" then .[0] else (.until // .[0]) end' "$EV/force_enter.json" | head -1)
    delta=$(( entered_until - $(node_height) ))
    [ "$delta" -ge $(( FORCE_DURATION - 20 )) ] && [ "$delta" -le $(( FORCE_DURATION + 20 )) ] || problems="$problems; EnteredUntil is $delta blocks ahead, expected ~$FORCE_DURATION"
    wait_blocks 3; h1=$(node_height)
    [ "$h1" -ge $(( h0 + 3 )) ] || problems="$problems; block production stalled ($h0 -> $h1)"
    hs=$(node_height); sent=$(send_user_tx "$EV/user_tx_entered.log"); wait_blocks 3; he=$(node_height)
    cf=$(call_filtered_between "$hs" "$he"); ms=$(midnight_events_between "$hs" "$he")
    state_set "$STATE_DIR/safe_mode.env" DRILL_FROM "$hs"
    if [ "$cf" = ERR ] || [ "$ms" = ERR ]; then problems="$problems; blocks $hs-$he not fully read (evidence/safe_mode/*_${hs}_${he}.err)"
    elif [ "$cf" -ge 1 ] && [ "$ms" = 0 ]; then notes="; the refused transaction was included as a failed extrinsic (node#2205)"
    elif [ "$ms" != 0 ]; then problems="$problems; a user transaction was applied in safe mode"
    else problems="$problems; no CallFiltered extrinsic in $hs-$he (sender: $sent)"; fi
    force_exit "$EV/force_exit" || problems="$problems; force_exit FAILED twice, safe mode is still entered: EXIT IT BY HAND"
    chain_check safe-mode > "$EV/safe_mode_after.json" 2>/dev/null
    [ "$(jq -r .isEntered "$EV/safe_mode_after.json" 2>/dev/null)" = false ] || problems="$problems; still entered after force_exit"
    hs=$(node_height); sent=$(send_user_tx "$EV/user_tx_exited.log"); wait_blocks 3; he=$(node_height)
    ms=$(midnight_events_between "$hs" "$he")
    local blocked=""
    case "$sent" in
        toolkit-ok|sdk-ok) [ "$ms" != ERR ] && [ "$ms" -ge 1 ] || problems="$problems; the post-exit transfer left no midnight event ($ms)" ;;
        sdk-failed) grep -q StateRootMismatch "$EV/user_tx_exited.log" && [ "$(client_custom_error "$EV/user_tx_exited.log.sdk")" = 170 ] \
                        && blocked=1 || problems="$problems; no sender could submit a transfer after force_exit ($sent)" ;;
        *) problems="$problems; no sender could submit a transfer after force_exit ($sent)" ;;
    esac
    if [ -n "$problems" ]; then t_fail "${problems#; }$notes"
    elif [ -n "$blocked" ]; then t_warn "safe mode entered until head+$delta, a user transaction refused, force_exit passed; not verified that a transaction lands afterwards: the toolkit cannot replay past the refused extrinsic, and the SDK wallet's DUST view comes from an indexer that counted it (midnight-indexer#1604)$notes"
    else t_pass "entered until head+$delta, blocks $h0 -> $h1, user transaction refused, force_exit, user transaction accepted$notes"; fi
}
c_toolkit_after() {
    [ "$SAFE_MODE_DRILL" = 1 ] || { t_skip "no drill"; return; }
    if tk_chain_ro -q show-wallet --seed "$SEED_1" > "$EV/toolkit_replay.log" 2>&1; then t_pass "show-wallet replays the whole chain"
    elif grep -q StateRootMismatch "$EV/toolkit_replay.log"; then t_warn "the toolkit stops replaying at the block with the CallFiltered extrinsic (StateRootMismatch): it applies a transaction the node did not (a toolkit limitation, not filed)"
    else t_fail "$(last_line_of "$EV/toolkit_replay.log")"; fi
}
c_indexer_after() {
    [ -n "$INDEXER_GQL" ] || { t_skip "no indexer answering"; return; }
    state_load "$STATE_DIR/safe_mode.env"
    local from="${DRILL_FROM:-${FORK_HEIGHT:-1}}" head blocks="" f t h r applied="" recorded="" why
    head=$(node_height); f=$from
    while [ "$f" -le "$head" ]; do
        t=$(( f + 39 )); [ "$t" -gt "$head" ] && t=$head
        why=$(scan_events "$f" "$t" system "$EV/indexer_scan.json") || { t_fail "$why"; return; }
        blocks="$blocks $(jq -r ".blocks[]? | select([.events[]? | $CALL_FILTERED] | length > 0) | .height" "$EV/indexer_scan.json" | tr '\n' ' ')"
        f=$(( t + 1 ))
    done
    blocks=$(echo "$blocks" | tr -s ' ' | sed 's/^ //; s/ $//')
    [ -n "$blocks" ] || { t_skip "no block with a CallFiltered extrinsic since #$from"; return; }
    indexer_catch_up 180 >/dev/null || true
    for h in $blocks; do
        r=$(gql_query "{ block(offset: { height: $h }) { transactions { hash ... on RegularTransaction { transactionResult { status } } } } }")
        echo "$r" > "$EV/indexer_block_$h.json"
        recorded="$recorded $h=$(echo "$r" | jq -r '[.data.block.transactions[]? | .transactionResult.status // "n/a"] | join(",")')"
        echo "$r" | jq -e '[.data.block.transactions[]? | select(.transactionResult.status == "SUCCESS")] | length > 0' >/dev/null 2>&1 && applied="$applied $h"
    done
    [ -z "$applied" ] && t_pass "blocks with CallFiltered:$blocks; indexer records:$recorded" \
        || t_known "the indexer recorded SUCCESS in block(s)$applied for a transaction the node rejected (midnight-indexer#1604):$recorded"
}
t_check SAFE-3 "node#2079" "governance drill: force_enter, a user transaction is refused, force_exit, a user transaction lands" c_drill
t_check SAFE-4 "node#2079" "the toolkit still replays the chain after the drill" c_toolkit_after
t_check SAFE-5 "HF-12" "the indexer did not record the refused transaction as applied" c_indexer_after

t_finish
