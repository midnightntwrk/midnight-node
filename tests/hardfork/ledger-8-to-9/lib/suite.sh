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

_SUITE_LIB="$(dirname "${BASH_SOURCE[0]}")"
source "$_SUITE_LIB/target.sh"
source "$_SUITE_LIB/results.sh"
source "$_SUITE_LIB/chain.sh"
source "$_SUITE_LIB/dapps.sh"
source "$_SUITE_LIB/clients.sh"
require_cmd docker jq curl python3 xxd git || exit 2

require_era() {  # l8|l9; sets SPEC_NOW
    SPEC_NOW=$(spec_version)
    [ -n "$SPEC_NOW" ] || cannot_run "node RPC $NODE_HTTP not answering"
    case "$1" in
        l8) [ "$SPEC_NOW" -lt "$LEDGER9_SPEC_FLOOR" ] || cannot_run "the chain is on spec $SPEC_NOW: forked already" ;;
        l9) [ "$SPEC_NOW" -ge "$LEDGER9_SPEC_FLOOR" ] || cannot_run "the chain is on spec $SPEC_NOW: not forked yet" ;;
    esac
}
require_indexer() { indexer_resolve --probe || cannot_run "no indexer answering at $INDEXER_BASE"; }
require_baseline() { [ -s "$PRE_FORK_STATE" ] || cannot_run "no $PRE_FORK_STATE: run the ledger-8 baseline first"; }

# Checks more than one table runs.
c_health() {  # [min blocks in 60 s: fewer is a WARN]
    local h n; h=$(chain_health 60) || { t_fail "$h"; return; }
    n=$(echo "$h" | grep -oE '\(\+[0-9]+\)' | head -1 | tr -dc 0-9)
    [ "${n:-0}" -ge "${1:-0}" ] && t_pass "$h" || t_warn "$h: fewer blocks than 6 s slots predict"
}
c_tx() {  # <log name> <generate-txs single-tx args...>
    local log="$EV/$1.log"; shift
    tk_tx generate-txs single-tx "$@" -d "$NODE_WS" > "$log" 2>&1 && t_pass "landed" || t_fail "$(last_line_of "$log")"
}
fetch_all() {  # <log>: every block; a fresh cache on local-env or with RUN_FULL_FETCH=1
    if [ "$TARGET" = local ] || [ "${RUN_FULL_FETCH:-0}" = 1 ]; then
        rm -rf "$EV/fetch_cache"; mkdir -p "$EV/fetch_cache"
        tk fetch --fetch-cache "redb:$EV/fetch_cache/fetch.db" -s "$NODE_WS" > "$1" 2>&1
    else tk_chain_ro fetch > "$1" 2>&1; fi
}
c_node_vs_indexer() {  # <fork height> [snapshot dir]: only the contracts it holds
    local a label n=0 bad="" known="" out
    for a in $(registry_addresses); do
        [ -z "${2:-}" ] || [ -s "$2/contracts/$a.txt" ] || continue
        n=$((n + 1)); label=$(registry_label "$a")
        out=$(contract_state_node_vs_indexer "$a" "$EV/nvi_$a") && continue
        if indexer_1605 "$a" "$EV/nvi_$a" "$1"; then known="$known $label"; else bad="$bad $label($out)"; fi
    done
    [ "$n" -gt 0 ] || { t_skip "no contracts recorded"; return; }
    if [ -n "$bad" ]; then t_fail "$bad${known:+; pre-fork state served for:$known}"
    elif [ -n "$known" ]; then t_known "the indexer serves the last pre-fork state, data unchanged, for contracts untouched since the fork (midnight-indexer#1605):$known"
    else t_pass "$n contracts: identical state bytes from node and indexer"; fi
}
