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

# Tables SNAP / SNAPDIFF: read-only. `pre` records the state the fork must preserve right
# before the runtime upgrade; `post` re-reads it on ledger 9 and compares, before anything
# else changes it. The ledger-9 toolkit reads both sides where it can.
#
#   snapshot.sh pre|post
set -e
source "$(dirname "${BASH_SOURCE[0]}")/../lib/suite.sh"

MODE="${1:-}"; case "$MODE" in pre|post) ;; *) echo "usage: snapshot.sh pre|post" >&2; exit 2 ;; esac
SNAP="$STATE_DIR/snapshot"; EV="$EVIDENCE_DIR/snapshot_$MODE"
mkdir -p "$SNAP/wallets" "$SNAP/contracts" "$EV/wallets" "$EV/contracts"
use_toolkit l9
require_indexer
state_load "$PRE_FORK_STATE"

night_pools() {  # <out>: "pool NIGHT" for every line of the supply breakdown
    tk_chain_ro show-night-pools > "$1.raw" 2>&1 || return 1
    sed $'s/\x1b\\[[0-9;]*m//g' "$1.raw" | sed -nE 's/^([A-Za-z_]+)[^:]*: +([0-9.]+) NIGHT$/\1 \2/p' > "$1"
    grep -q '^TOTAL ' "$1"
}
# The supply is fixed at 24B NIGHT. Ledger 9 adds a bridge_receiving pool that the toolkit's
# TOTAL leaves out, so on a network with bridge traffic it may read lower, never higher.
supply_problem() {  # <pools file>
    local t; t=$(awk '$1 == "TOTAL" { print $2 }' "$1")
    [ "$t" = 24000000000.000000 ] && return 1
    if [ "$TARGET" = network ] && python3 -c "import sys; sys.exit(0 if float('$t') < 24e9 else 1)"; then echo "WARN TOTAL $t below 24B (pending bridge_receiving?)"
    else echo "FAIL TOTAL $t, the supply is 24000000000.000000"; fi
}
wallet_row() {  # <seed> <dir> <n> -> "unshielded shielded dust_total dust_sources"
    wallet_snapshot "$1" "$2/seed$3.wallet.json" || return 1
    dust_snapshot "$1" "$2/seed$3.dust.json" || return 1
    echo "$(wallet_night_unshielded "$2/seed$3.wallet.json") $(wallet_night_shielded "$2/seed$3.wallet.json") $(jq -r .total "$2/seed$3.dust.json") $(jq -r '.source | length' "$2/seed$3.dust.json")"
}
# Prices float with block usage and the serialised blob changes with any field: leave them out.
ledger_params() {  # <out>; the ledger-9 toolkit cannot read parameters from a ledger-8 node
    tk show-ledger-parameters -r "$NODE_WS" > "$1" 2>&1 && grep -q overall_price "$1" && return 0
    ( use_toolkit l8; tk show-ledger-parameters -r "$NODE_WS" > "$1" 2>&1 ) && grep -q overall_price "$1"
}

if [ "$MODE" = pre ]; then
    require_era l8
    t_table SNAP "Pre-fork snapshot on $NETWORK_NAME at $(utc_now)"

    c_chain() {
        local fh fn; fh=$(rpc chain_getFinalizedHead | jq -r .result); fn=$(hex2dec "$(rpc chain_getHeader "[\"$fh\"]" | jq -r .result.number)")
        rpc midnight_ledgerStateRoot "[\"$fh\"]" | jq -c .result > "$SNAP/ledger_root.json"
        rpc midnight_zswapStateRoot "[\"$fh\"]" | jq -c .result > "$SNAP/zswap_root.json"
        {
            printf 'SNAP_UTC=%q\nSNAP_GENESIS=%q\nSNAP_FINALIZED_HEIGHT=%q\nSNAP_FINALIZED_HASH=%q\n' "$(utc_now)" "$(genesis_hash)" "$fn" "$fh"
            printf 'SNAP_SPEC=%q\nSNAP_LEDGER=%q\nSNAP_METADATA_SHA256=%q\n' "$(spec_version)" "$(ledger_version)" "$(rpc state_getMetadata | jq -r .result | sha256_of)"
        } > "$SNAP/chain.env"
        [ -s "$SNAP/ledger_root.json" ] && [ "$(cat "$SNAP/ledger_root.json")" != null ] \
            && t_pass "finalized #$fn $fh; ledger and zswap roots recorded" || t_fail "no ledger state root for #$fn"
    }
    c_params() { ledger_params "$SNAP/params.txt" && t_pass "$(grep -c ':' "$SNAP/params.txt") lines" || t_fail "$(last_line_of "$SNAP/params.txt")"; }
    c_pools() {
        night_pools "$SNAP/night_pools.txt" || { t_fail "$(last_line_of "$SNAP/night_pools.txt.raw")"; return; }
        local p; p=$(supply_problem "$SNAP/night_pools.txt")
        case "$p" in FAIL*) t_fail "${p#FAIL }" ;; WARN*) t_warn "${p#WARN }" ;; *) t_pass "$(tr '\n' ';' < "$SNAP/night_pools.txt")" ;; esac
    }
    c_wallets() {
        local i s row bad=""
        : > "$SNAP/wallets.tsv"
        for i in 1 2 3 4; do
            s="SEED_$i"
            if row=$(wallet_row "${!s}" "$SNAP/wallets" "$i"); then printf '%s %s\n' "$i" "$row" >> "$SNAP/wallets.tsv"
            else bad="$bad seed$i"; fi
        done
        [ -z "$bad" ] || { t_fail "unreadable:$bad"; return; }
        local now; now=$(awk '$1 == 3 { print $2, $3 }' "$SNAP/wallets.tsv")
        night_kept "${SEED3_NIGHT:-}" "$now" || { t_fail "seed 3 NIGHT ${SEED3_NIGHT:-unrecorded} at HF-PRE-1, $now now: the ledger-8 transaction's inputs may be spent"; return; }
        t_pass "$(tr '\n' ';' < "$SNAP/wallets.tsv") seed 3 unchanged since HF-PRE-1"
    }
    c_contracts() {
        local a n=0 bad="" nvi=""
        source "$SNAP/chain.env" 2>/dev/null; [ -n "${SNAP_FINALIZED_HASH:-}" ] || { t_fail "no snapshot block (SNAP-CHAIN)"; return; }
        for a in $(registry_addresses); do
            n=$((n + 1))
            semantic_snapshot "$a" "$SNAP/contracts/$a.txt" && contract_data_at "$a" "$SNAP_FINALIZED_HASH" "$SNAP/contracts/$a" > "$SNAP/contracts/$a.data" \
                || bad="$bad $(registry_label "$a")"
            contract_state_node_vs_indexer "$a" "$EV/contracts/$a" > /dev/null || nvi="$nvi $(registry_label "$a")"
        done
        [ "$n" -gt 0 ] || { t_skip "no contracts in the registry"; return; }
        if [ -n "$bad" ]; then t_fail "unreadable:$bad"
        elif [ -n "$nvi" ]; then t_fail "node and indexer serve different state for:$nvi"
        else t_pass "$n contracts recorded; node and indexer serve identical state for all"; fi
    }
    c_txs() {
        tx_inclusion_check "$SNAP/txs.tsv" "$EVIDENCE_DIR"/*/*.log "$DAPPS_DIR"/*/*send.log > "$EV/txs.txt" \
            && t_pass "$(cat "$EV/txs.txt")" || t_fail "$(cat "$EV/txs.txt")"
    }
    t_check SNAP-CHAIN "HF-01" "genesis, finalized block, ledger and zswap state roots, runtime and metadata hash" c_chain
    t_check SNAP-PARAMS "HF-09" "ledger parameters" c_params
    t_check SNAP-POOLS "HF-04" "every NIGHT pool; the supply totals 24B" c_pools
    t_check SNAP-WALLETS "HF-04, HF-05" "NIGHT and DUST of the four test wallets; seed 3 unspent since HF-PRE-1" c_wallets
    t_check SNAP-CONTRACTS "HF-02, HF-12" "entry points, authority and data of every registry contract; node and indexer agree" c_contracts
    t_check SNAP-TXS "HF-12" "every recorded transaction pinned to its block" c_txs
    t_finish
    exit $?
fi

[ -s "$SNAP/chain.env" ] || cannot_run "no pre-fork snapshot in $SNAP (snapshot.sh pre)"
source "$SNAP/chain.env"
require_era l9
FORK_HEIGHT=$(fork_height "$SNAP_SPEC") || FORK_HEIGHT=""
t_table SNAPDIFF "Pre-fork snapshot of $SNAP_UTC diffed on ledger 9 ($NETWORK_NAME)"

c_sd_genesis() { local g; g=$(genesis_hash); [ "$g" = "$SNAP_GENESIS" ] && t_pass "$g" || t_fail "$SNAP_GENESIS -> $g"; }
c_sd_versions() {
    local s l; s=$(spec_version); l=$(ledger_version)
    if [ "$s" = "$L9_EXPECTED_SPEC" ] && [[ "$l" == *"$L9_EXPECTED_LEDGER_SUBSTR"* ]]; then t_pass "spec $SNAP_SPEC -> $s, ledger $SNAP_LEDGER -> $l"
    else t_fail "spec $s (expected $L9_EXPECTED_SPEC), ledger '$l'"; fi
}
c_sd_roots() {
    local l z
    l=$(rpc midnight_ledgerStateRoot "[\"$SNAP_FINALIZED_HASH\"]" | jq -c .result); z=$(rpc midnight_zswapStateRoot "[\"$SNAP_FINALIZED_HASH\"]" | jq -c .result)
    if [ "$l" = "$(cat "$SNAP/ledger_root.json")" ] && [ "$z" = "$(cat "$SNAP/zswap_root.json")" ]; then t_pass "block #$SNAP_FINALIZED_HEIGHT roots served unchanged"
    else t_fail "roots of #$SNAP_FINALIZED_HEIGHT changed (ledger $(cat "$SNAP/ledger_root.json" | cut -c1-24) -> $(echo "$l" | cut -c1-24))"; fi
}
c_sd_params() {
    ledger_params "$EV/params.txt" || { t_fail "$(last_line_of "$EV/params.txt")"; return; }
    param_lines "$SNAP/params.txt" > "$EV/params.pre"; param_lines "$EV/params.txt" > "$EV/params.post"
    local gone new; gone=$(comm -23 "$EV/params.pre" "$EV/params.post" | tr '\n' ';' | cut -c1-200); new=$(comm -13 "$EV/params.pre" "$EV/params.post" | wc -l | tr -d ' ')
    if ! grep -q min_block_price "$EV/params.txt"; then t_fail "min_block_price missing after the fork"
    elif [ -n "$gone" ]; then t_warn "pre-fork fields changed or gone: $gone ($new new lines)"
    else t_pass "every pre-fork field unchanged (prices aside); $new new lines incl. $(grep -m1 -o 'min_block_price[^,]*' "$EV/params.txt")"; fi
}
# Nothing of the suite moves NIGHT between the snapshot and this read; on a network other
# wallets may.
c_sd_pools() {
    night_pools "$EV/night_pools.txt" || { t_fail "$(last_line_of "$EV/night_pools.txt.raw")"; return; }
    local p moved; p=$(supply_problem "$EV/night_pools.txt")
    moved=$(diff "$SNAP/night_pools.txt" "$EV/night_pools.txt" | grep '^[<>]' | tr '\n' ';' | cut -c1-200)
    case "$p" in FAIL*) t_fail "${p#FAIL }" ;; WARN*) t_warn "${p#WARN }${moved:+; moved: $moved}" ;;
        *) if [ -z "$moved" ]; then t_pass "every pool unchanged; TOTAL 24B"
           elif [ "$TARGET" = network ]; then t_warn "TOTAL 24B; pools moved, other wallets transact on a shared network: $moved"
           else t_fail "pools moved: $moved"; fi ;;
    esac
}
c_sd_night() {
    local i s pre row bad="" moved=""
    : > "$EV/wallets.tsv"
    for i in 1 2 3 4; do
        s="SEED_$i"; pre=$(awk -v n="$i" '$1 == n { print $2, $3 }' "$SNAP/wallets.tsv")
        row=$(wallet_row "${!s}" "$EV/wallets" "$i") || { bad="$bad seed$i"; continue; }
        night_kept "$pre" "$(echo "$row" | cut -d' ' -f1-2)" || moved="$moved seed$i($pre -> $(echo "$row" | cut -d' ' -f1-2))"
        echo "$i $row" >> "$EV/wallets.tsv"
    done
    if [ -n "$bad" ]; then t_fail "unreadable on ledger 9:$bad"
    elif [ -n "$moved" ]; then t_fail "NIGHT moved since the snapshot:$moved"
    else t_pass "unshielded and shielded NIGHT of all four wallets $([ "$TARGET" = local ] && echo identical || echo 'kept (none lower)')"; fi
}
c_sd_dust() {
    [ -s "$EV/wallets.tsv" ] || { t_skip "wallets unreadable (SD-WALLETS-1)"; return; }
    local i tot src bad="" detail=""
    while read -r i _ _ tot src; do
        detail="$detail seed$i=$src/$tot"
        if [ "$i" = "${CNIGHT_SEED_INDEX:-0}" ]; then [ "$src" -ge 1 ] || bad="$bad seed$i(cNIGHT-backed, no source replayed)"
        elif [ "$src" != 0 ] || [ "$tot" != 0 ]; then bad="$bad seed$i($src sources, $tot)"; fi
    done < "$EV/wallets.tsv"
    [ -z "$bad" ] && t_pass "native DUST reset to zero by design (sources/total:$detail)" || t_fail "$bad"
}
c_sd_contracts() {
    [ -n "$FORK_HEIGHT" ] || { t_fail "no verifiable fork block for spec $SNAP_SPEC"; return; }
    local a label r n=0 bad="" touched=""
    for a in $(registry_addresses); do
        [ -s "$SNAP/contracts/$a.txt" ] || continue
        n=$((n + 1)); label=$(registry_label "$a")
        semantic_snapshot "$a" "$EV/contracts/$a.txt" || { bad="$bad $label(unreadable)"; continue; }
        diff "$SNAP/contracts/$a.txt" "$EV/contracts/$a.txt" > "$EV/contracts/$a.diff" || bad="$bad $label(entry points or authority)"
        r=$(contract_data_across_fork "$a" "$FORK_HEIGHT" "$EV/contracts/$a") || { bad="$bad $label(data: $r)"; continue; }
        [ "$(cut -d' ' -f2 "$SNAP/contracts/$a.data")" = "$(contract_data "$EV/contracts/$a.fork_pre.hex" | cut -d' ' -f2)" ] || touched="$touched $label"
    done
    [ "$n" -gt 0 ] || { t_skip "no contracts in the snapshot"; return; }
    if [ -n "$bad" ]; then t_fail "$bad"
    elif [ -n "$touched" ]; then t_warn "the fork kept every contract's data, but a transaction changed it between the snapshot and block #$((FORK_HEIGHT - 1)):$touched"
    else t_pass "$n contracts: entry points, authority and data identical from the snapshot through fork block #$FORK_HEIGHT"; fi
}
c_sd_txs() {
    : > "$EV/txs_moved.txt"
    [ -s "$SNAP/txs.tsv" ] || { t_skip "no recorded transactions"; return; }
    local h pre now moved=0 n=0
    while IFS=$'\t' read -r h pre; do
        [ "$h" = tx_hash ] && continue
        n=$((n + 1))
        now=$(gql_query "{ transactions(offset: { hash: \"$h\" }) { block { height } } }" | jq -r '.data.transactions[0].block.height // "MISSING"')
        [ "$now" = "$pre" ] || { moved=$((moved + 1)); echo "$h $pre -> $now" >> "$EV/txs_moved.txt"; }
    done < "$SNAP/txs.tsv"
    [ "$moved" = 0 ] && t_pass "all $n transactions at their pre-fork blocks" || t_fail "$moved of $n moved or missing (evidence/snapshot_post/txs_moved.txt)"
}
t_check SD-CHAIN-1 "HF-01" "genesis unchanged" c_sd_genesis
t_check SD-CHAIN-2 "HF-01" "spec and ledger version moved to ledger 9" c_sd_versions
t_check SD-CHAIN-3 "HF-01" "the pre-fork block's ledger and zswap roots are served unchanged" c_sd_roots
t_check SD-PARAMS-1 "HF-09" "pre-fork ledger parameters unchanged; ledger-9 fields present" c_sd_params
t_check SD-POOLS-1 "HF-04" "every NIGHT pool unchanged; the supply totals 24B" c_sd_pools
t_check SD-WALLETS-1 "HF-05" "NIGHT of every test wallet unchanged" c_sd_night
t_check SD-WALLETS-2 "HF-04" "native DUST reset to zero (a cNIGHT-backed wallet keeps a replayed source)" c_sd_dust
t_check SD-CONTRACTS-1 "HF-02, HF-03" "every registry contract: entry points, authority and data identical; the data unchanged across the fork block" c_sd_contracts
t_check SD-CONTRACTS-2 "HF-12" "node and indexer serve the same state for every registry contract" c_node_vs_indexer "$FORK_HEIGHT" "$SNAP"
t_check SD-TXS-1 "HF-12" "every recorded transaction is still at its block on the indexer" c_sd_txs
t_finish
