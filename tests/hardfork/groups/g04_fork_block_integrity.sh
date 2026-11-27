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

# Group 4: fork block integrity.
#
# Was anything lost, did the chain pause, did any transaction get stuck at the fork block?
# Three parts, at three moments:
#
#   g04_fork_block_integrity.sh record    once every node runs the new binary, right before
#                                         the runtime upgrade: balances, supply, contract
#                                         data and state roots are recorded. Read-only.
#   g04_fork_block_integrity.sh crosser   start a minute or two before the upgrade is
#                                         submitted: the crosser's wallet sends a transfer
#                                         every few seconds until a few minutes after the
#                                         fork. Returns when it is done.
#   g04_fork_block_integrity.sh verify    first thing after the fork, before anything else
#                                         changes the state: compares with the record.
#
# What the fork changes on purpose is group 5.
set -e
source "$(dirname "${BASH_SOURCE[0]}")/../lib/suite.sh"

PART="${1:-}"
case "$PART" in
    record|crosser|verify) ;;
    *) echo "usage: g04_fork_block_integrity.sh record|crosser|verify" >&2; exit 2 ;;
esac

evidence_dir "g4_$PART"
recall_state
use_ledger_side new          # the new toolkit reads both sides of the fork
use_wallets genesis
require_indexer
require_group2_record
RECORD="$STATE_DIR/before_fork"
mkdir -p "$RECORD/wallets" "$RECORD/contracts" "$RECORD/contract_balances" "$EV/wallets" "$EV/contracts"
CHECK_NAME_PREFIX=""
RECORDED_WALLETS=(SENDER RECEIVER HOLDER SPARE)

# --- Reading the state, the same way before and after the fork ---------------------------
read_night_pools() {  # <out>: one "pool amount" line for every pool of the supply
    tk_chain_ro show-night-pools > "$1.raw" 2>&1 || return 1
    sed $'s/\x1b\\[[0-9;]*m//g' "$1.raw" | sed -nE 's/^([A-Za-z_]+)[^:]*: +([0-9.]+) NIGHT$/\1 \2/p' > "$1"
    grep -q '^TOTAL ' "$1"
}

TOTAL_SUPPLY="24000000000.000000"
# On a network with bridge traffic NIGHT in transit is left out of the total, so there it
# may read lower, never higher.
supply_problem() {  # <pools file> -> nothing, or "WARN ..." or "FAIL ..."
    local total; total=$(awk '$1 == "TOTAL" { print $2 }' "$1")
    [ "$total" = "$TOTAL_SUPPLY" ] && return 0
    if [ "$TARGET" = network ] && python3 -c "import sys; sys.exit(0 if float(sys.argv[1]) < float(sys.argv[2]) else 1)" "$total" "$TOTAL_SUPPLY"; then
        echo "WARN the total is $total, below $TOTAL_SUPPLY (NIGHT in transit over the bridge?)"
    else
        echo "FAIL the total is $total, the supply is $TOTAL_SUPPLY"
    fi
}

read_wallet() {  # <role> <directory> -> "unshielded shielded dust_total dust_sources"
    local seed="${1}_SEED"
    wallet_snapshot "${!seed}" "$2/$1.wallet.json" || return 1
    dust_snapshot "${!seed}" "$2/$1.dust.json" || return 1
    echo "$(wallet_night_unshielded "$2/$1.wallet.json") $(wallet_night_shielded "$2/$1.wallet.json")" \
        "$(jq -r .total "$2/$1.dust.json") $(jq -r '.source | length' "$2/$1.dust.json")"
}

# The new toolkit cannot read the parameters from a node still on the old ledger.
read_ledger_parameters() {  # <out>
    tk show-ledger-parameters -r "$NODE_WS" > "$1" 2>&1 && grep -q overall_price "$1" && return 0
    ( use_toolkit old; tk show-ledger-parameters -r "$NODE_WS" > "$1" 2>&1 ) && grep -q overall_price "$1"
}

contracts_holding_funds() {  # addresses of the games and the vault deployed before the fork
    awk -F'\t' '$5 == "old" && ($2 ~ /^game-/ || $2 == "vault") { print $4 }' "$REGISTRY"
}

# =========================================================================================
if [ "$PART" = record ]; then
    require_chain_on old
    require_clients
    group_start G4 "Fork block integrity on $NETWORK_NAME: the record before the runtime upgrade"

    record_chain() {
        local finalized_hash finalized_height_now
        finalized_hash=$(rpc chain_getFinalizedHead | jq -r .result)
        finalized_height_now=$(hex2dec "$(rpc chain_getHeader "[\"$finalized_hash\"]" | jq -r .result.number)")
        rpc midnight_ledgerStateRoot "[\"$finalized_hash\"]" | jq -c .result > "$RECORD/ledger_root.json"
        rpc midnight_zswapStateRoot "[\"$finalized_hash\"]" | jq -c .result > "$RECORD/zswap_root.json"
        [ "$(cat "$RECORD/ledger_root.json")" != null ] || { echo "no ledger state root for finalized #$finalized_height_now"; return 1; }
        remember RECORDED_AT "$(utc_now)"
        remember RECORDED_GENESIS "$(genesis_hash)"
        remember RECORDED_BLOCK_HEIGHT "$finalized_height_now"
        remember RECORDED_BLOCK_HASH "$finalized_hash"
    }

    record_pools() {
        local problem
        read_night_pools "$RECORD/night_pools.txt" || { echo "the supply is unreadable: $(last_line_of "$RECORD/night_pools.txt.raw")"; return 1; }
        problem=$(supply_problem "$RECORD/night_pools.txt")
        [[ "$problem" != FAIL* ]] || { echo "${problem#FAIL }"; return 1; }
    }

    record_wallets() {
        local role balances holder_now
        : > "$RECORD/wallets.tsv"
        for role in "${RECORDED_WALLETS[@]}"; do
            balances=$(read_wallet "$role" "$RECORD/wallets") || { echo "the $role wallet is unreadable"; return 1; }
            echo "$role $balances" >> "$RECORD/wallets.tsv"
        done
        holder_now=$(awk '$1 == "HOLDER" { print $2, $3 }' "$RECORD/wallets.tsv")
        night_kept "${HOLDER_NIGHT_BEFORE_FORK:-}" "$holder_now" \
            || { echo "the holder's NIGHT went ${HOLDER_NIGHT_BEFORE_FORK:-unrecorded} -> $holder_now since group 2: the unsent transaction's inputs may be spent"; return 1; }
    }

    record_contracts() {
        local address label
        for address in $(registry_addresses old); do
            label=$(registry_label "$address")
            semantic_snapshot "$address" "$RECORD/contracts/$address.txt" || { echo "contract $label is unreadable"; return 1; }
            contract_data_at "$address" "$RECORDED_BLOCK_HASH" "$RECORD/contracts/$address" > "$RECORD/contracts/$address.data" \
                || { echo "the data of contract $label is unreadable"; return 1; }
        done
        for address in $(contracts_holding_funds); do
            contract_balances "$address" > "$RECORD/contract_balances/$address.json" \
                || { echo "the balances of $(registry_label "$address") are unreadable"; return 1; }
        done
    }

    check_state_recorded() {
        local problem
        problem=$(record_chain) || { fail "$problem"; return; }
        recall_state
        read_ledger_parameters "$RECORD/ledger_parameters.txt" || { fail "the ledger parameters are unreadable: $(last_line_of "$RECORD/ledger_parameters.txt")"; return; }
        problem=$(record_pools) || { fail "$problem"; return; }
        problem=$(record_wallets) || { fail "$problem"; return; }
        problem=$(record_contracts) || { fail "$problem"; return; }
        tx_inclusion_check "$RECORD/transactions.tsv" "$EVIDENCE_DIR"/*/*.log "$DAPPS_DIR"/*/*send.log > "$EV/transactions.txt" \
            || { fail "$(cat "$EV/transactions.txt")"; return; }
        pass "at finalized #$RECORDED_BLOCK_HEIGHT: state roots, ledger parameters, the supply and its pools, ${#RECORDED_WALLETS[@]} wallets, $(registry_addresses old | wc -l | tr -d ' ') contracts, $(( $(wc -l < "$RECORD/transactions.tsv") - 1 )) transactions; the holder has not spent"
    }

    bullet "Record balances, supply, contract data and state roots"
    check state-recorded "the state right before the runtime upgrade is recorded" check_state_recorded
    group_finish
    exit $?
fi

# =========================================================================================
if [ "$PART" = crosser ]; then
    require_chain_on old
    require_clients
    export CLIENT_SEED="$CROSSER_SEED"
    # How the crosser's wallet behaves (clients/wallet_check.mjs soak).
    export LOAD_INTERVAL_SECS=5          # one transfer every five seconds
    export LOAD_AFTER_FORK_SECS=180      # keeps going this long after the fork
    export LOAD_REGISTER_AFTER_SECS=60   # re-registers for DUST if nothing lands this long after the fork
    export LOAD_TX_TIMEOUT_SECS=240      # a transfer with no verdict after this long counts as timed out
    if [ "$TARGET" = local ]; then export LOAD_MAX_SECS=2400; else export LOAD_MAX_SECS=7200; fi   # gives up if no fork comes
    export MN_TIMEOUT_SECS=$(( LOAD_MAX_SECS + 900 ))
    CROSSER_LOG="$EV/crosser.json"
    FORK_TIMESTAMP=""

    group_start G4 "Fork block integrity on $NETWORK_NAME: one wallet transacting across the runtime upgrade"

    # What the crosser's log says about one question (lib/soak.py): "PASS|WARN|FAIL detail".
    crosser_verdict() {  # <prefork|inflight|recovered>
        [ -n "$FORK_TIMESTAMP" ] || { skip "the crosser did not run across the fork (G4-crosser-ran-across-fork)"; return; }
        local verdict
        verdict=$(python3 "$SUITE_DIR/lib/soak.py" "$CROSSER_LOG" "$FORK_HEIGHT" "$FORK_TIMESTAMP" "$NEW_SPEC_FLOOR" "$OLD_LEDGER" "$NEW_LEDGER" "$1")
        case "$verdict" in
            PASS*) pass "${verdict#PASS }" ;;
            WARN*) warn "${verdict#WARN }" ;;
            *)     fail "${verdict#FAIL }" ;;
        esac
    }

    check_crosser_ran_across_fork() {
        local first_block; first_block=$(node_height)
        note "one transfer every ${LOAD_INTERVAL_SECS}s until ${LOAD_AFTER_FORK_SECS}s after the fork; progress in $EV/crosser.err"
        client_run "$EV/crosser" wallet_check.mjs soak || { fail "the wallet process failed: $(client_error "$EV/crosser")"; return; }
        [ "$(spec_version)" -ge "$NEW_SPEC_FLOOR" ] || { fail "no fork within ${LOAD_MAX_SECS}s (the runtime spec is $(spec_version))"; return; }
        FORK_HEIGHT=$(fork_height "$SPEC_BEFORE_FORK") || { fail "the fork block cannot be verified (the RPC node must keep old runtime state)"; return; }
        indexer_catch_up 180 > /dev/null || true
        FORK_TIMESTAMP=$(gql_query "{ block(offset: { height: $FORK_HEIGHT }) { timestamp } }" | jq -r '.data.block.timestamp // empty')
        [ -n "$FORK_TIMESTAMP" ] || { fail "the indexer has no timestamp for fork block #$FORK_HEIGHT"; return; }
        pass "$(jq '.attempts | length' "$CROSSER_LOG") transfers attempted over blocks #$first_block..#$(node_height), across fork block #$FORK_HEIGHT"
    }

    check_nothing_left_pending() {
        [ -n "$FORK_TIMESTAMP" ] || { skip "the crosser did not run across the fork (G4-crosser-ran-across-fork)"; return; }
        wait_blocks 3 > /dev/null || true
        local node pending still_in_pool="" nodes_asked=0 unresolved
        for node in $(alive_nodes); do
            nodes_asked=$(( nodes_asked + 1 ))
            pending=$(rpc_at "${node#*=}" author_pendingExtrinsics | jq -r '.result | length' 2> /dev/null)
            [ "${pending:-unreadable}" = 0 ] || still_in_pool="$still_in_pool ${node%%=*}: ${pending:-unreadable};"
        done
        unresolved=$(jq -r '[.finalPending[] | select(. == "Submitted")] | length' "$CROSSER_LOG" 2> /dev/null)
        if [ -n "$still_in_pool" ]; then fail "transactions left in a node's pool:$still_in_pool"
        elif [ "${unresolved:-0}" != 0 ]; then fail "the wallet still counts $unresolved transaction(s) as submitted, with no verdict"
        else pass "the pools of $nodes_asked nodes are empty and the wallet holds no unresolved transaction"; fi
    }

    bullet "Transactions in flight"
    check crosser-ran-across-fork          "one wallet process ran from ledger $OLD_LEDGER through the fork block into ledger $NEW_LEDGER, without a restart" check_crosser_ran_across_fork
    check transfers-before-fork-landed     "every transfer that finished before the fork block landed"                    crosser_verdict prefork
    check in-flight-transfers-got-verdict  "every old-ledger transfer the fork block caught got a verdict"                   crosser_verdict inflight
    check nothing-left-pending             "nothing stays pending, in any node's pool or in the wallet"                   check_nothing_left_pending
    check crosser-transacts-on-new-ledger  "the same wallet transacts on ledger $NEW_LEDGER, re-registering for DUST if it must" crosser_verdict recovered
    group_finish
    exit $?
fi

# =========================================================================================
# verify
require_chain_on new
require_clients
[ -n "${RECORDED_BLOCK_HASH:-}" ] || cannot_run "nothing was recorded before the runtime upgrade (g04_fork_block_integrity.sh record)"
FORK_HEIGHT=""

group_start G4 "Fork block integrity on $NETWORK_NAME: the record of $RECORDED_AT compared on ledger $NEW_LEDGER"

skip_without_fork_block() {
    [ -n "$FORK_HEIGHT" ] && return 1
    skip "the fork block was not found (G4-fork-block-found)"
}

# Accepted only when the old runtime is seen at the block before it.
check_fork_block_found() {
    if FORK_HEIGHT=$(fork_height "$SPEC_BEFORE_FORK"); then
        pass "block #$FORK_HEIGHT is the first on the new runtime; block #$(( FORK_HEIGHT - 1 )) still reports spec $SPEC_BEFORE_FORK"
    else
        FORK_HEIGHT=""
        fail "no block where spec $SPEC_BEFORE_FORK changes can be verified (the RPC node must keep old runtime state)"
    fi
}

check_genesis_unchanged() {
    local genesis; genesis=$(genesis_hash)
    if [ "$genesis" = "$RECORDED_GENESIS" ]; then pass "$genesis"
    else fail "the genesis hash went $RECORDED_GENESIS -> $genesis"; fi
}

# Nothing of the suite moves NIGHT between the record and this read; on a network other
# wallets may send to ours.
check_wallet_balances_unchanged() {
    local role recorded balances unreadable="" moved=""
    : > "$EV/wallets.tsv"
    for role in "${RECORDED_WALLETS[@]}"; do
        recorded=$(awk -v role="$role" '$1 == role { print $2, $3 }' "$RECORD/wallets.tsv")
        balances=$(read_wallet "$role" "$EV/wallets") || { unreadable="$unreadable $role"; continue; }
        echo "$role $balances" >> "$EV/wallets.tsv"
        night_kept "$recorded" "$(cut -d' ' -f1-2 <<< "$balances")" || moved="$moved $role ($recorded -> $(cut -d' ' -f1-2 <<< "$balances"));"
    done
    # Group 8 judges the unsent transaction against the holder's wallet as it is right now.
    [ ! -s "$EV/wallets/HOLDER.wallet.json" ] || cp "$EV/wallets/HOLDER.wallet.json" "$STATE_DIR/holder_wallet_after_fork.json"
    if [ -n "$unreadable" ]; then fail "unreadable on ledger $NEW_LEDGER:$unreadable"
    elif [ -n "$moved" ]; then fail "NIGHT moved since the record:$moved"
    else pass "unshielded and shielded NIGHT of ${RECORDED_WALLETS[*]} as recorded"; fi
}

check_supply_and_pools_unchanged() {
    local problem moved
    read_night_pools "$EV/night_pools.txt" || { fail "the supply is unreadable: $(last_line_of "$EV/night_pools.txt.raw")"; return; }
    problem=$(supply_problem "$EV/night_pools.txt")
    moved=$(diff "$RECORD/night_pools.txt" "$EV/night_pools.txt" | grep '^[<>]' | tr '\n' ';' | cut -c1-200)
    if [[ "$problem" == FAIL* ]]; then fail "${problem#FAIL }"
    elif [[ "$problem" == WARN* ]]; then warn "${problem#WARN }${moved:+; pools moved: $moved}"
    elif [ -z "$moved" ]; then pass "every pool as recorded; the total is $TOTAL_SUPPLY"
    elif [ "$TARGET" = network ]; then warn "the total is $TOTAL_SUPPLY; pools moved, as other wallets transact on a shared network: $moved"
    else fail "pools moved: $moved"; fi
}

# The fork re-encodes contract state, so contracts are compared by entry points, authority
# and the hash of their decoded data.
check_contract_state_unchanged() {
    skip_without_fork_block && return
    local address label comparison total=0 changed="" changed_before_fork=""
    for address in $(registry_addresses old); do
        [ -s "$RECORD/contracts/$address.txt" ] || continue
        total=$(( total + 1 )); label=$(registry_label "$address")
        semantic_snapshot "$address" "$EV/contracts/$address.txt" || { changed="$changed $label (unreadable);"; continue; }
        diff "$RECORD/contracts/$address.txt" "$EV/contracts/$address.txt" > "$EV/contracts/$address.diff" \
            || changed="$changed $label (entry points or authority);"
        comparison=$(contract_data_across_fork "$address" "$FORK_HEIGHT" "$EV/contracts/$address") \
            || { changed="$changed $label (data: $comparison);"; continue; }
        [ "$(cut -d' ' -f2 "$RECORD/contracts/$address.data")" = "$(contract_data "$EV/contracts/$address.fork_pre.hex" | cut -d' ' -f2)" ] \
            || changed_before_fork="$changed_before_fork $label"
    done
    if [ "$total" = 0 ]; then skip "no contract was recorded"
    elif [ -n "$changed" ]; then fail "$changed"
    elif [ -n "$changed_before_fork" ]; then warn "the fork kept every contract's data, but a transaction changed it between the record and block #$(( FORK_HEIGHT - 1 )):$changed_before_fork"
    else pass "$total contracts: entry points, authority and data identical from the record through fork block #$FORK_HEIGHT"; fi
}

check_contract_balances_unchanged() {
    local address now recorded total=0 changed="" report=""
    for address in $(contracts_holding_funds); do
        [ -s "$RECORD/contract_balances/$address.json" ] || continue
        total=$(( total + 1 )); recorded=$(cat "$RECORD/contract_balances/$address.json")
        now=$(contract_balances "$address") || { changed="$changed $(registry_label "$address") (unreadable);"; continue; }
        [ "$now" = "$recorded" ] || changed="$changed $(registry_label "$address") (recorded $recorded, now $now);"
        report="$report $(registry_label "$address") $recorded;"
    done
    if [ "$total" = 0 ]; then skip "no contract holding funds was recorded"
    elif [ -n "$changed" ]; then fail "$changed"
    else pass "as recorded:$report"; fi
}

check_ledger_parameters_unchanged() {
    local gone added
    read_ledger_parameters "$EV/ledger_parameters.txt" || { fail "the ledger parameters are unreadable: $(last_line_of "$EV/ledger_parameters.txt")"; return; }
    param_lines "$RECORD/ledger_parameters.txt" > "$EV/parameters_before.txt"
    param_lines "$EV/ledger_parameters.txt" > "$EV/parameters_after.txt"
    gone=$(comm -23 "$EV/parameters_before.txt" "$EV/parameters_after.txt" | tr '\n' ';' | cut -c1-200)
    added=$(comm -13 "$EV/parameters_before.txt" "$EV/parameters_after.txt" | wc -l | tr -d ' ')
    if [ -n "$gone" ]; then warn "parameters from before the fork changed or are gone: $gone ($added new lines)"
    else pass "every parameter from before the fork is unchanged (prices aside, they follow block usage); $added lines are new"; fi
}

check_pre_fork_state_roots_served() {
    local ledger_root zswap_root
    ledger_root=$(rpc midnight_ledgerStateRoot "[\"$RECORDED_BLOCK_HASH\"]" | jq -c .result)
    zswap_root=$(rpc midnight_zswapStateRoot "[\"$RECORDED_BLOCK_HASH\"]" | jq -c .result)
    if [ "$ledger_root" = "$(cat "$RECORD/ledger_root.json")" ] && [ "$zswap_root" = "$(cat "$RECORD/zswap_root.json")" ]; then
        pass "the ledger and zswap state roots of block #$RECORDED_BLOCK_HEIGHT are served as recorded"
    else
        fail "the state roots of block #$RECORDED_BLOCK_HEIGHT changed: ledger root $(cut -c1-24 "$RECORD/ledger_root.json") -> $(cut -c1-24 <<< "$ledger_root")"
    fi
}

check_transactions_still_in_their_blocks() {
    [ -s "$RECORD/transactions.tsv" ] || { skip "no transaction was recorded"; return; }
    local hash recorded_height height total=0 moved=0
    : > "$EV/transactions_moved.txt"
    while IFS=$'\t' read -r hash recorded_height; do
        [ "$hash" = tx_hash ] && continue
        total=$(( total + 1 ))
        height=$(gql_query "{ transactions(offset: { hash: \"$hash\" }) { block { height } } }" | jq -r '.data.transactions[0].block.height // "missing"')
        [ "$height" = "$recorded_height" ] && continue
        moved=$(( moved + 1 ))
        echo "$hash $recorded_height -> $height" >> "$EV/transactions_moved.txt"
    done < "$RECORD/transactions.tsv"
    if [ "$moved" = 0 ]; then pass "all $total recorded transactions are in the blocks they were in"
    else fail "$moved of $total moved or are missing ($EV/transactions_moved.txt)"; fi
}

# The indexer's block timestamps around the fork block.
check_block_pace_through_fork() {
    skip_without_fork_block && return
    local height timestamp previous="" gap longest=0 longest_at=""
    for (( height = FORK_HEIGHT - 5; height <= FORK_HEIGHT + 5; height++ )); do
        timestamp=$(gql_query "{ block(offset: { height: $height }) { timestamp } }" | jq -r '.data.block.timestamp // empty')
        [ -n "$timestamp" ] || { fail "the indexer has no timestamp for block #$height"; return; }
        if [ -n "$previous" ]; then
            gap=$(( (timestamp - previous) / 1000 ))
            if [ "$gap" -gt "$longest" ]; then longest=$gap; longest_at=$height; fi
        fi
        previous=$timestamp
    done
    if [ "$longest" -le 18 ]; then pass "the longest gap between blocks #$(( FORK_HEIGHT - 5 ))..#$(( FORK_HEIGHT + 5 )) is ${longest}s, before #$longest_at"
    else warn "a ${longest}s gap before block #$longest_at, more than three 6-second slots: the upgrade or the migration held block production"; fi
}

check_nodes_agree_on_blocks() {
    skip_without_fork_block && return
    multi_node || { skip "only one node can be asked (EXTRA_RPC_NODES adds more)"; return; }
    local first=$(( FORK_HEIGHT - 1 )) tip; tip=$(node_height)
    if sweep_forks "$first" "$tip" > "$EV/blocks_agreement.txt" 2>&1; then pass "every node has the same block hashes #$first..#$tip"
    else fail "$(grep -m1 -E 'FORK|NO AGREEMENT' "$EV/blocks_agreement.txt")"; fi
}

check_nodes_agree_on_state() {
    skip_without_fork_block && return
    multi_node || { skip "only one node can be asked"; return; }
    local finalized; finalized=$(finalized_height)
    if check_state_agreement "$(block_hash_at "$NODE_HTTP" "$FORK_HEIGHT")" > "$EV/state_agreement_fork_block.txt" 2>&1 \
        && check_state_agreement "$(block_hash_at "$NODE_HTTP" "$finalized")" > "$EV/state_agreement_finalized.txt" 2>&1; then
        pass "state, ledger and zswap roots agree at fork block #$FORK_HEIGHT and at finalized #$finalized"
    else
        fail "$(cat "$EV"/state_agreement_*.txt | grep -m1 -E 'DIVERGENCE|NO AGREEMENT')"
    fi
}

check_all_nodes_on_new_runtime() {
    local node spec_now on_another_spec="" total=0 progress
    spec_now=$(spec_version)
    for node in "${NODES[@]}"; do
        total=$(( total + 1 ))
        [ "$(spec_version_at "${node#*=}")" = "$spec_now" ] || on_another_spec="$on_another_spec ${node%%=*}"
    done
    [ -z "$on_another_spec" ] || { fail "unreachable or on another runtime spec:$on_another_spec"; return; }
    progress=$(wait_finality_progress 2 90) || { fail "every node is on spec $spec_now, but $progress"; return; }
    pass "$total of $total nodes on spec $spec_now; $progress"
}

bullet "Accept the fork height only once the old version is seen at the block before it"
check fork-block-found                    "the fork block is found and verified"                                  check_fork_block_found

bullet "Nothing is lost"
check genesis-unchanged                   "the genesis hash"                                                      check_genesis_unchanged
check wallet-balances-unchanged           "the NIGHT of every test wallet"                                        check_wallet_balances_unchanged
check supply-and-pools-unchanged          "the total supply and every pool of it"                                 check_supply_and_pools_unchanged
check contract-state-unchanged            "every contract: entry points, authority, and its data across the fork block" check_contract_state_unchanged
check contract-balances-unchanged         "the funds held by the games and the vault"                             check_contract_balances_unchanged
check ledger-parameters-unchanged         "the ledger parameters"                                                 check_ledger_parameters_unchanged
check pre-fork-state-roots-served         "the state roots of the recorded block are still served unchanged"      check_pre_fork_state_roots_served
check transactions-still-in-their-blocks  "every recorded transaction is still in its block"                      check_transactions_still_in_their_blocks

bullet "The chain did not pause"
check block-pace-through-fork             "blocks kept their pace through the fork block"                         check_block_pace_through_fork
check nodes-agree-on-blocks               "every node agrees on the fork block and the blocks after it"           check_nodes_agree_on_blocks
check nodes-agree-on-state                "every node agrees on the state at the fork block and at the finalized block" check_nodes_agree_on_state
check all-nodes-on-new-runtime            "every node runs the new runtime and finality advances"                 check_all_nodes_on_new_runtime

group_finish
