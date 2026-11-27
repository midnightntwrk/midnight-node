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

# Group 5, the ledger 8 -> 9 fork's migration (node#2012).
#
# The fork resets the DUST that native NIGHT generated and replays the DUST generation that
# is backed by cNIGHT. Wallets re-register to generate DUST again. Until they have, nothing
# pays fees, and a registration is paid from DUST accrued since the fork: on a network start
# this group a few minutes after the fork.
#
# Sourced by groups/g05_state_migration.sh, which has set the scene.

# The wallet whose DUST is backed by cNIGHT, if the network has one (local-env observes no
# cNIGHT). Named by role in the network settings: CNIGHT_WALLET=SPARE.
CNIGHT_WALLET="${CNIGHT_WALLET:-}"
NATIVE_WALLETS=()
for role in SENDER RECEIVER HOLDER SPARE; do
    [ "$role" = "$CNIGHT_WALLET" ] || NATIVE_WALLETS+=("$role")
done

# --- What changes, changes as designed ---------------------------------------------------
check_native_dust_reset() {
    local role seed sources total report="" not_reset=""
    for role in "${NATIVE_WALLETS[@]}"; do
        seed="${role}_SEED"
        dust_snapshot "${!seed}" "$EV/dust_after_fork_$role.json" || { not_reset="$not_reset $role (unreadable);"; continue; }
        sources=$(jq '.source | length' "$EV/dust_after_fork_$role.json"); total=$(jq -r .total "$EV/dust_after_fork_$role.json")
        report="$report $role: $sources sources, total $total;"
        [ "$total" = 0 ] || not_reset="$not_reset $role;"
    done
    if [ -z "$not_reset" ]; then pass "$report"
    else fail "not reset:$not_reset ($report)"; fi
}

check_cnight_wallet_keeps_dust() {
    [ -n "$CNIGHT_WALLET" ] || { skip "no cNIGHT-backed test wallet (CNIGHT_WALLET); local-env observes no cNIGHT"; return; }
    local seed="${CNIGHT_WALLET}_SEED" out="$EV/dust_after_fork_cnight.json" sources
    dust_snapshot "${!seed}" "$out" || { fail "the DUST balance is unreadable"; return; }
    sources=$(jq '.source | length' "$out")
    if [ "$sources" -ge 1 ] && big_gt "$(jq -r .total "$out")" 0; then pass "$CNIGHT_WALLET: $sources cNIGHT-backed source(s), total $(jq -r .total "$out")"
    else fail "$CNIGHT_WALLET: no replayed cNIGHT-backed DUST source"; fi
}

check_indexer_shows_dust_reset() {
    local tip protocol_version dust_tree_end spec_now
    tip=$(indexer_block); spec_now=$(spec_version)
    protocol_version=$(jq -r '.protocolVersion // empty' <<< "$tip"); dust_tree_end=$(jq -r '.dustGenerationEndIndex // empty' <<< "$tip")
    [ "$protocol_version" = "$spec_now" ] || { fail "the indexer's protocolVersion is $protocol_version, the runtime spec is $spec_now"; return; }
    if [ -n "$dust_tree_end" ] && [ "$dust_tree_end" -gt 0 ] && [ "$dust_tree_end" -lt "${DUST_TREE_END_BEFORE_FORK:-0}" ]; then
        pass "protocolVersion ${INDEXER_PROTOCOL_VERSION_BEFORE_FORK:-?} -> $protocol_version; the DUST generation tree went ${DUST_TREE_END_BEFORE_FORK:-?} -> $dust_tree_end entries: reset, then refilled by re-registrations"
    elif [ "$TARGET" = network ] && [ -n "$dust_tree_end" ]; then
        warn "the DUST generation tree went ${DUST_TREE_END_BEFORE_FORK:-?} -> $dust_tree_end entries: other wallets re-register on a shared network, so the reset may be hidden"
    else
        fail "the DUST generation tree went ${DUST_TREE_END_BEFORE_FORK:-?} -> ${dust_tree_end:-null} entries, expected fewer than before and more than none"
    fi
}

# --- Users recover what was reset ----------------------------------------------------------
# The holder's registration is built, held for a minute, then sent: the next check needs it.
HOLD_REGISTRATION_SECS=60
HOLDER_REGISTRATION_BUILT_AT=""
check_wallets_reregister_for_dust() {
    local role seed problems="" held_tx="$EV/registration_HOLDER.mn" held_log="$EV/registration_HOLDER.log"
    for role in SENDER RECEIVER; do
        seed="${role}_SEED"
        tk_tx generate-txs register-dust-address --wallet-seed "${!seed}" -d "$NODE_WS" > "$EV/registration_$role.log" 2>&1 \
            || problems="$problems $role: $(last_line_of "$EV/registration_$role.log" 120);"
    done
    rm -f "$held_tx"
    if tk_chain generate-txs --dest-file "$held_tx" register-dust-address --wallet-seed "$HOLDER_SEED" > "$held_log" 2>&1 && [ -s "$held_tx" ]; then
        HOLDER_REGISTRATION_BUILT_AT=$(date -u '+%s')
        sleep "$HOLD_REGISTRATION_SECS"
        tk_send "$held_tx" >> "$held_log" 2>&1 || problems="$problems HOLDER: $(last_line_of "$held_log" 120);"
    else
        problems="$problems HOLDER: $(last_line_of "$held_log" 120);"
    fi
    if [ -z "$problems" ]; then pass "sender, receiver and holder re-registered, each paying the fee from DUST accrued since the fork"
    else fail "$problems"; fi
}

check_dust_generates_again() {
    wait_blocks 3 > /dev/null || true
    local growth
    if growth=$(dust_growth_check "$SENDER_SEED" "$EV/dust_generating_again"); then pass "$growth; the sender's total is $(jq -r .total "$EV/dust_generating_again_s2.json")"
    else fail "no DUST source of the sender grew: $growth"; fi
}

# New DUST must start at the time of the block the registration landed in, not at the time
# the transaction was built.
check_held_registration_uses_block_time() {
    [ -n "$HOLDER_REGISTRATION_BUILT_AT" ] || { skip "the holder's registration was not built (G5-wallets-reregister-for-dust)"; return; }
    local hash block block_time dust_start out="$EV/dust_of_held_registration.json"
    hash=$(tx_hashes_in "$EV/registration_HOLDER.log" | tail -1)
    [ -n "$hash" ] || { fail "no transaction hash in $EV/registration_HOLDER.log"; return; }
    indexer_catch_up > /dev/null || true
    block=$(gql_query "{ transactions(offset: { hash: \"$hash\" }) { block { height timestamp } } }" | jq -c '.data.transactions[0].block // empty')
    [ -n "$block" ] || { fail "the indexer does not have registration $hash"; return; }
    block_time=$(( $(jq -r .timestamp <<< "$block") / 1000 ))
    dust_snapshot "$HOLDER_SEED" "$out" || { fail "the holder's DUST balance is unreadable ($out.err)"; return; }
    dust_start=$(jq '[.generation_infos[].dust_output.ctime] | max // empty' "$out")
    [ -n "$dust_start" ] || { fail "the holder has no DUST output after its registration"; return; }
    if [ "$dust_start" = "$block_time" ] && [ "$dust_start" -ge $(( HOLDER_REGISTRATION_BUILT_AT + HOLD_REGISTRATION_SECS - 5 )) ]; then
        pass "built at $HOLDER_REGISTRATION_BUILT_AT, sent ${HOLD_REGISTRATION_SECS}s later; its DUST starts at $dust_start, the time of block #$(jq -r .height <<< "$block")"
    else
        fail "its DUST starts at $dust_start; block #$(jq -r .height <<< "$block") is at $block_time; built at $HOLDER_REGISTRATION_BUILT_AT and held ${HOLD_REGISTRATION_SECS}s"
    fi
}

# The same recovery through the wallet SDK. Re-registering UTXOs the toolkit just registered
# is refused by the node (Custom error 138), as in midnight-wallet#415.
check_sdk_reregisters_for_dust() {
    local out="$EV/sdk_dust_registration" node_error
    clients_ready > "$EV/clients_ready.log" 2>&1 || { fail "npm ci in $CLIENT_PACKAGES_DIR failed ($EV/clients_ready.log)"; return; }
    if client_run "$out" wallet_check.mjs dust-register; then
        pass "the wallet SDK registered $(jq -r .utxos "$out.json") NIGHT UTXOs"
        return
    fi
    node_error=$(grep -m1 -oE 'Custom error: [0-9]+' "$out.err" | grep -oE '[0-9]+$')
    if [ "$node_error" = 138 ]; then
        known "the node refused it with Custom error 138: these UTXOs are registered already, by the toolkit above (midnight-wallet#415, closed). The crosser's wallet in group 4 is the SDK's own re-registration"
    else
        fail "$(client_error "$out")"
    fi
}

# --- The migration completes -----------------------------------------------------------------
check_migration_ran_on_every_node() {
    logs_available || { skip "no node logs on this target (KUBE_CONTEXT reads them)"; return; }
    local node completed applied skipped report="" without=""
    for node in $(log_nodes); do
        node_logs "$node" > "$EV/log_$node.txt"
        completed=$(grep -oE 'DustReapplyCompleted: dust generation replay complete, [0-9]+ applied, [0-9]+ skipped' "$EV/log_$node.txt" | tail -1)
        if [ -z "$completed" ] || ! grep -q DustReapplyStarted "$EV/log_$node.txt"; then without="$without $node"; continue; fi
        applied=$(grep -oE '[0-9]+ applied' <<< "$completed" | grep -oE '[0-9]+'); skipped=$(grep -oE '[0-9]+ skipped' <<< "$completed" | grep -oE '[0-9]+')
        report="$report $node: $applied applied, $skipped skipped;"
    done
    if [ -z "$without" ]; then pass "the DUST replay started and completed:$report"
    else fail "no DUST replay logged on:$without"; fi
}

check_same_migration_cost_everywhere() {
    logs_available || { skip "no node logs on this target (KUBE_CONTEXT reads them)"; return; }
    local node distinct_costs
    : > "$EV/migration_cost.txt"
    for node in $(log_nodes); do
        node_logs "$node" | grep -oE 'translation complete in [0-9]+ step\(s\), [^,]+, [0-9]+ps synthetic cost' | tail -1 | sed "s/^/$node: /" >> "$EV/migration_cost.txt" || true
    done
    distinct_costs=$(grep -oE '[0-9]+ps synthetic cost' "$EV/migration_cost.txt" | sort -u | wc -l | tr -d ' ')
    if [ "$distinct_costs" = 1 ]; then pass "$(head -1 "$EV/migration_cost.txt" | cut -d: -f2-), the same on $(wc -l < "$EV/migration_cost.txt" | tr -d ' ') nodes"
    else fail "$distinct_costs different costs across the nodes ($EV/migration_cost.txt)"; fi
}

bullet "What changes, changes as designed"
check native-dust-reset                   "DUST generated by native NIGHT is reset to zero for every test wallet"        check_native_dust_reset
check cnight-wallet-keeps-dust            "a wallet whose DUST is backed by cNIGHT keeps a replayed source"              check_cnight_wallet_keeps_dust

bullet "Users can recover what was reset the way the design says"
check wallets-reregister-for-dust         "wallets re-register for DUST, paying from DUST accrued since the fork"        check_wallets_reregister_for_dust
check dust-generates-again                "DUST generates again after the re-registration"                               check_dust_generates_again
check held-registration-uses-block-time   "a registration held before it is sent generates from its block's time"        check_held_registration_uses_block_time
check sdk-reregisters-for-dust            "the wallet SDK re-registers for DUST"                                         check_sdk_reregisters_for_dust
check indexer-shows-dust-reset            "the indexer shows the DUST generation tree reset and refilled"                check_indexer_shows_dust_reset

bullet "The migration completes, with the same cost on every validator"
check migration-ran-on-every-node         "the DUST replay started and completed on every node"                          check_migration_ran_on_every_node
check same-migration-cost-everywhere      "every node reports the same migration cost for the fork block"                check_same_migration_cost_everywhere

bullet "It holds on real data"
not_covered migration-on-mainnet-state    "the migration's duration and cost on mainnet state"                           "needs an ephemeral environment started from mainnet state"
