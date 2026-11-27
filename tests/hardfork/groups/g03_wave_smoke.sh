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

# Group 3: smoke test after each binary wave.
#
# Old and new binaries run side by side while the runtime is still the old ledger: is the
# chain still healthy and usable? If this fails, the next wave waits.
#
#   g03_wave_smoke.sh <wave number>
#
# EXPECTED_UPGRADED_NODES: how many of the nodes we can ask should run the new binary by now.
# The holder's wallet is left alone: it backs the unsent old-ledger transaction.
set -e
source "$(dirname "${BASH_SOURCE[0]}")/../lib/suite.sh"

WAVE="${1:?usage: g03_wave_smoke.sh <wave number>}"
evidence_dir "g3_wave$WAVE"
rm -f "${EV:?}"/*
recall_state
use_ledger_side old
use_wallets genesis
require_indexer
require_group2_record
export DAPP_FILE_PREFIX=prefork
CHECK_NAME_PREFIX="wave$WAVE-"
SMOKE_STARTED=$(date -u '+%Y-%m-%dT%H:%M:%SZ')

group_start G3 "Smoke test after binary wave $WAVE on $NETWORK_NAME (the runtime is still ledger $OLD_LEDGER)"

check_runtime_still_old() {
    local spec; spec=$(spec_version)
    if [ "$spec" = "$SPEC_BEFORE_FORK" ]; then pass "spec $spec"
    else fail "the runtime spec is $spec before the runtime upgrade, expected $SPEC_BEFORE_FORK"; fi
}

check_upgraded_nodes_running() {
    local versions total upgraded
    versions=$(node_versions)
    total=$(grep -c . <<< "$versions" || true); upgraded=$(grep -cF "=$NEW_EXPECTED_NODE_VERSION" <<< "$versions" || true)
    versions=${versions//$'\n'/, }
    if [ "$total" = 0 ]; then fail "no node answers"
    elif [ -n "${EXPECTED_UPGRADED_NODES:-}" ] && [ "$upgraded" -lt "$EXPECTED_UPGRADED_NODES" ]; then
        fail "$upgraded of $total nodes run $NEW_EXPECTED_NODE_VERSION, expected $EXPECTED_UPGRADED_NODES: $versions"
    elif [ "$upgraded" = 0 ]; then
        warn "none of the nodes we can ask runs $NEW_EXPECTED_NODE_VERSION yet (they may not be validators; EXPECTED_UPGRADED_NODES makes this a check): $versions"
    else
        pass "$upgraded of $total nodes run $NEW_EXPECTED_NODE_VERSION, each on its existing database: $versions"
    fi
}

check_nodes_agree() {
    multi_node || { skip "only one node can be asked"; return; }
    local finalized first_block
    finalized=$(finalized_height); first_block=$(( finalized > 20 ? finalized - 20 : 1 ))
    if check_state_agreement "$(block_hash_at "$NODE_HTTP" "$finalized")" > "$EV/agreement.txt" 2>&1 \
        && sweep_forks "$first_block" "$finalized" >> "$EV/agreement.txt" 2>&1; then
        pass "old and new nodes agree on the state roots at finalized #$finalized and on the blocks #$first_block..#$finalized"
    else
        fail "$(grep -m1 -E 'FORK|DIVERGENCE|NO AGREEMENT' "$EV/agreement.txt")"
    fi
}

check_transfer() {
    local problem
    problem=$(send_night transfer --source-seed "$SENDER_SEED" --unshielded-amount 1 --destination-address "$RECEIVER_ADDRESS") \
        && pass "landed" || fail "$problem"
}

# The contracts must also stay what group 2 recorded.
check_contract_call() {
    local label address problem changed=""
    [ -n "$(registry_address store-a)" ] || { skip "no contract store-a (group 2)"; return; }
    problem=$(store_contract_call store-a "$SENDER_SEED" check) || { fail "$problem"; return; }
    for label in store-a store-b; do
        address=$(registry_address "$label")
        [ -n "$address" ] || continue
        semantic_snapshot "$address" "$EV/state_$label.txt" && diff -q "$(recorded_contract_state "$address")" "$EV/state_$label.txt" > /dev/null \
            || changed="$changed $label"
    done
    if [ -z "$changed" ]; then pass "a check on store-a landed; entry points and authority of store-a and store-b as recorded"
    else fail "entry points or authority changed, or unreadable:$changed"; fi
}

check_dapp_call() {
    local counter; counter=$(registry_address counter)
    [ -n "$counter" ] || { skip "no counter dApp (group 2)"; return; }
    if dapp_call counter "$SENDER_SEED" "$counter" increment; then
        cp "$(dapp_log counter increment_send)" "$EV/dapp_call_send.log"
        pass "counter.increment landed"
    else fail "$(dapp_err counter increment)"; fi
}

check_indexer_keeps_up() {
    local problem report
    problem=$(indexer_catch_up 180) || { fail "$problem"; return; }
    if report=$(tx_inclusion_check "$EV/tx_inclusion.tsv" "$EV"/*.log); then pass "at the tip; $report"
    else fail "$report"; fi
}

logs_since_smoke_started() {  # <node or pod>
    if [ "$TARGET" = local ]; then docker logs --since "$SMOKE_STARTED" "$1" 2>&1
    else _kubectl --request-timeout=60s logs "$1" --all-containers --since-time="$SMOKE_STARTED" 2>&1; fi
}

# A validator restarted within the slot of the block it authored last tries once to author
# in that slot again. The runtime refuses ("Slot must increase") and the node carries on.
AUTHORED_TWICE_IN_ONE_SLOT='Slot must increase'

check_logs_clean() {
    logs_available || { skip "no node logs on this target (KUBE_CONTEXT reads them)"; return; }
    local node log errors known_panics restart_panics with_errors="" with_known_panics="" with_restart_panics="" total=0 services=""
    [ "$TARGET" = local ] && services="chain-indexer wallet-indexer indexer-api"
    for node in $(log_nodes) $services; do
        total=$(( total + 1 )); log="$EV/log_$node.txt"
        logs_since_smoke_started "$node" > "$log" || true
        errors=$(grep -acE ' ERROR |"level":"ERROR"|panicked' "$log" || true)
        known_panics=0
        [ -z "${KNOWN_OLD_BINARY_PANIC:-}" ] || known_panics=$(grep -acE "$KNOWN_OLD_BINARY_PANIC" "$log" || true)
        restart_panics=$(grep -aA1 'panicked' "$log" | grep -ac "$AUTHORED_TWICE_IN_ONE_SLOT" || true)
        [ "$errors" -le $(( known_panics + restart_panics )) ] || with_errors="$with_errors $node ($(( errors - known_panics - restart_panics )) lines)"
        [ "$known_panics" = 0 ] || with_known_panics="$with_known_panics $node ($known_panics)"
        [ "$restart_panics" = 0 ] || with_restart_panics="$with_restart_panics $node ($restart_panics)"
    done
    if [ "$total" = 0 ]; then fail "no node log could be read"
    elif [ -n "$with_errors" ]; then fail "ERROR or panic lines since the smoke started:$with_errors ($EV/log_*.txt)"
    elif [ -n "$with_restart_panics" ]; then warn "a restarted validator tried to author in the slot of its own last block, was refused ($AUTHORED_TWICE_IN_ONE_SLOT) and carried on:$with_restart_panics; no other ERROR or panic line"
    elif [ -n "$with_known_panics" ]; then known "only the old binary's storage panic ($KNOWN_OLD_BINARY_PANIC_ISSUE):$with_known_panics"
    else pass "no ERROR or panic line in the logs of $total nodes and services"; fi
}

bullet "The runtime is still the old ledger"
check runtime-still-old       "the runtime spec has not changed"                                       check_runtime_still_old

bullet "Every node that was upgraded started on its existing database and configuration"
check upgraded-nodes-running  "the upgraded nodes answer and report the new version"                   check_upgraded_nodes_running

bullet "Blocks are produced and finalized at the usual pace"
check chain-healthy           "blocks are produced and finalized over 60 seconds"                      check_chain_healthy

bullet "Old and new nodes agree on the same blocks"
check nodes-agree             "every node has the same blocks and state roots"                         check_nodes_agree

bullet "A transfer, a contract call and a dApp call still go through"
check transfer                "an unshielded NIGHT transfer, sender to receiver"                       check_transfer
check contract-call           "a call on a contract from group 2; the contracts are unchanged"         check_contract_call
check dapp-call               "counter.increment on the counter from group 2"                          check_dapp_call

bullet "The indexer keeps up"
check indexer-keeps-up        "the indexer is at the tip and has every transaction of this smoke"      check_indexer_keeps_up

# Last, so the logs cover everything the smoke did.
bullet "Every node that was upgraded started on its existing database and configuration"
check logs-clean              "no ERROR or panic line in the node and indexer logs during this smoke"  check_logs_clean

group_finish
