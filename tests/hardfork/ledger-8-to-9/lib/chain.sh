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

# Node RPC, indexer, wallet and contract helpers.
#
# The ledger root is part of Substrate storage, so a divergent ledger migration changes the
# block hash: equal hashes across nodes at a height mean the ledgers agree too.

# --- RPC ----------------------------------------------------------------------------
rpc_at() {  # <url> <method> [params-json]
    curl -s -m 20 -X POST "$1" -H 'Content-Type: application/json' \
        -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"$2\",\"params\":${3:-[]}}"
}
rpc() { rpc_at "$NODE_HTTP" "$@"; }

node_url() { local n; for n in "${NODES[@]}"; do [ "${n%%=*}" = "$1" ] && { echo "${n#*=}"; return 0; }; done; return 1; }
best_height_at()      { hex2dec "$(rpc_at "$1" chain_getHeader | jq -r '.result.number // empty' 2>/dev/null)"; }
finalized_height_at() {
    local fh; fh=$(rpc_at "$1" chain_getFinalizedHead | jq -r '.result // empty' 2>/dev/null)
    [ -n "$fh" ] || { echo ""; return; }
    hex2dec "$(rpc_at "$1" chain_getHeader "[\"$fh\"]" | jq -r '.result.number // empty')"
}
block_hash_at()   { rpc_at "$1" chain_getBlockHash "[$2]" | jq -r '.result // empty'; }
state_root_at()   { rpc_at "$1" chain_getHeader "[\"$2\"]" | jq -r '.result.stateRoot // empty'; }
ledger_root_at()  { _root_rpc "$1" midnight_ledgerStateRoot "$2"; }
zswap_root_at()   { _root_rpc "$1" midnight_zswapStateRoot "$2"; }
_root_rpc() { rpc_at "$1" "$2" "[\"$3\"]" | jq -r 'if .result then (.result|tostring) else "ERR:"+(.error.message//"?") end'; }
spec_version_at() { rpc_at "$1" state_getRuntimeVersion | jq -r '.result.specVersion // empty'; }
node_version_at() { rpc_at "$1" system_version | jq -r '.result // empty'; }

node_height()      { best_height_at "$NODE_HTTP"; }
finalized_height() { finalized_height_at "$NODE_HTTP"; }
spec_version()     { spec_version_at "$NODE_HTTP"; }
node_version()     { node_version_at "$NODE_HTTP"; }
ledger_version()   { rpc midnight_ledgerVersion | jq -r '.result // empty'; }
genesis_hash()     { block_hash_at "$NODE_HTTP" 0; }

alive_nodes() {  # name=url of every node that answers
    local n
    for n in "${NODES[@]}"; do [ -n "$(best_height_at "${n#*=}")" ] && echo "$n"; done
}
multi_node() { [ "$(alive_nodes | wc -l | tr -d ' ')" -ge 2 ]; }
node_versions() {  # name=version of every node that answers
    local n; while read -r n; do [ -n "$n" ] && echo "${n%%=*}=$(node_version_at "${n#*=}")"; done < <(alive_nodes)
}
# Ledger parameters that must survive the fork: prices move with the chain, so they are out.
param_lines() { grep -E '^[[:space:]]*[a-z_]+: ' "$1" | sed -E 's/^[[:space:]]+//; s/,[[:space:]]*$//' | grep -viE 'price|_factor|serialized' | sort -u; }

wait_blocks() {  # <n> [timeout-s]; fails when the chain stalls
    local h target waited=0 limit=${2:-$(( $1 * 20 + 60 ))}
    h=$(node_height); [ -n "$h" ] || { echo "node unreachable"; return 1; }
    target=$(( h + $1 ))
    until h=$(node_height); [ "${h:-0}" -ge "$target" ]; do
        [ "$waited" -lt "$limit" ] || { echo "the chain stalled at #${h:-?}, waited ${limit}s for #$target"; return 1; }
        sleep 3; waited=$((waited + 3))
    done
}

chain_health() {  # <seconds>; fails when production or finality stalled
    local secs=${1:-60} h1 f1 h2 f2
    h1=$(node_height); f1=$(finalized_height); sleep "$secs"
    h2=$(node_height); f2=$(finalized_height)
    echo "over ${secs}s: height $h1 -> $h2 (+$(( ${h2:-0} - ${h1:-0} ))), finalized $f1 -> $f2 (+$(( ${f2:-0} - ${f1:-0} )))"
    [ "${h2:-0}" -gt "${h1:-0}" ] && [ "${f2:-0}" -gt "${f1:-0}" ]
}

# --- Cross-node agreement -----------------------------------------------------------
# Agreement needs MIN_AGREE_NODES answers: fewer is a failure ("NO AGREEMENT"), not a pass.
MIN_AGREE_NODES="${MIN_AGREE_NODES:-2}"
check_height_agreement() {  # <height>
    local height=$1 n h first="" forked=0 lines="" cnt=0
    [ -n "$height" ] || { echo "NO AGREEMENT: no height"; return 1; }
    while read -r n; do
        [ -n "$n" ] || continue
        h=$(block_hash_at "${n#*=}" "$height"); [ -n "$h" ] || continue
        cnt=$((cnt + 1)); lines+="  ${n%%=*} $h"$'\n'
        if [ -z "$first" ]; then first="$h"; elif [ "$h" != "$first" ]; then forked=1; fi
    done < <(alive_nodes)
    if [ "$forked" = 1 ]; then echo "FORK at height $height:"; printf '%s' "$lines"; return 1; fi
    [ "$cnt" -ge "$MIN_AGREE_NODES" ] || { echo "NO AGREEMENT at height $height: $cnt node(s) answered, $MIN_AGREE_NODES needed"; return 1; }
    echo "AGREE height $height = $first ($cnt nodes)"
}

# A root RPC error on one node is unavailability, not divergence; every root must still be
# compared on MIN_AGREE_NODES nodes.
check_state_agreement() {  # <block-hash>
    local bh=$1 n name url root v diverged=0 lines="" unavailable="" short=""
    local -A first=() count=()
    [ -n "$bh" ] || { echo "NO AGREEMENT: no block hash"; return 1; }
    while read -r n; do
        [ -n "$n" ] || continue
        name="${n%%=*}"; url="${n#*=}"
        for root in stateRoot ledgerStateRoot zswapStateRoot; do
            case $root in
                stateRoot)       v=$(state_root_at "$url" "$bh") ;;
                ledgerStateRoot) v=$(ledger_root_at "$url" "$bh") ;;
                zswapStateRoot)  v=$(zswap_root_at "$url" "$bh") ;;
            esac
            case "$v" in ""|ERR:*) unavailable+="  $name $root: ${v:-none}"$'\n'; continue ;; esac
            [ "$root" = stateRoot ] && lines+="  $name stateRoot=$v"$'\n'
            count[$root]=$(( ${count[$root]:-0} + 1 ))
            if [ -z "${first[$root]:-}" ]; then first[$root]=$v
            elif [ "$v" != "${first[$root]}" ]; then diverged=1; lines+="    $name $root differs"$'\n'; fi
        done
    done < <(alive_nodes)
    for root in stateRoot ledgerStateRoot zswapStateRoot; do
        [ "${count[$root]:-0}" -ge "$MIN_AGREE_NODES" ] || short+=" $root on ${count[$root]:-0},"
    done
    if [ "$diverged" = 1 ]; then echo "STATE DIVERGENCE at $bh:"
    elif [ -n "$short" ]; then echo "NO AGREEMENT at $bh: compared${short%,} node(s), $MIN_AGREE_NODES needed"
    else echo "STATE AGREE at $bh (stateRoot ${first[stateRoot]:0:18}..., ${count[stateRoot]} nodes)"; fi
    printf '%s' "$lines"
    [ -n "$unavailable" ] && { echo "  state RPC unavailable (not divergence):"; printf '%s' "$unavailable"; }
    [ "$diverged" = 0 ] && [ -z "$short" ]
}

sweep_forks() {  # <from> <to>
    local h rc=0
    [[ "${1:-}" =~ ^[0-9]+$ && "${2:-}" =~ ^[0-9]+$ ]] && [ "$1" -le "$2" ] || { echo "NO AGREEMENT: no block range [${1:-}, ${2:-}]"; return 1; }
    for h in $(seq "$1" "$2"); do
        check_height_agreement "$h" >/dev/null 2>&1 || { check_height_agreement "$h"; rc=1; }
    done
    [ "$rc" = 0 ] && echo "no forks in [$1,$2] across $(alive_nodes | wc -l | tr -d ' ') node(s)"
    return $rc
}

wait_finality_progress() {  # <blocks> <timeout-s>
    local start now waited=0
    start=$(finalized_height); [ -n "$start" ] || { echo "node unreachable"; return 1; }
    while [ "$waited" -lt "$2" ]; do
        now=$(finalized_height)
        [ -n "$now" ] && [ "$now" -ge $(( start + $1 )) ] && { echo "finality advanced $start -> $now"; return 0; }
        sleep 3; waited=$((waited + 3))
    done
    echo "finality stalled at $(finalized_height) (was $start) after $2 s"
    return 1
}

# --- Fork height --------------------------------------------------------------------
# Old runtime state must be readable: on a pruned RPC node every lookup fails, so nothing
# is guessed.
spec_at_height() {  # <height>
    local h s; h=$(block_hash_at "$NODE_HTTP" "$1"); [ -n "$h" ] || return 1
    s=$(rpc state_getRuntimeVersion "[\"$h\"]" | jq -r '.result.specVersion // empty'); [ -n "$s" ] && echo "$s"
}
is_fork_height() {  # <pre-fork spec> <height>: the old spec at height-1, another at height
    local before after
    [[ "${2:-}" =~ ^[0-9]+$ ]] && [ "$2" -ge 1 ] || return 1
    before=$(spec_at_height $(( $2 - 1 ))) && after=$(spec_at_height "$2") || return 1
    [ "$before" = "$1" ] && [ "$after" != "$1" ]
}
find_fork_height() {  # <pre-fork spec> -> the first block on the new runtime; fails when unsure
    local lo=1 hi mid s
    [ -n "${1:-}" ] || return 1
    hi=$(node_height); [ -n "$hi" ] || return 1
    while [ "$lo" -lt "$hi" ]; do
        mid=$(( (lo + hi) / 2 ))
        s=$(spec_at_height "$mid") || return 1
        if [ "$s" = "$1" ]; then lo=$(( mid + 1 )); else hi=$mid; fi
    done
    is_fork_height "$1" "$lo" && echo "$lo"
}
fork_height() {  # <pre-fork spec> -> the verified fork height; saves it to FORK_STATE
    local fh; fh=$(state_load "$FORK_STATE"; echo "${FORK_HEIGHT:-}")
    if [ -z "$fh" ] || ! is_fork_height "$1" "$fh"; then
        fh=$(find_fork_height "$1") || return 1
        state_set "$FORK_STATE" FORK_HEIGHT "$fh"
    fi
    echo "$fh"
}

# --- Raw extrinsics ----------------------------------------------------------------
# The node's own verdict on a transaction, without a client that could refuse it first.
bare_extrinsic() { python3 "$SUITE_DIR/lib/extrinsic.py" "$@"; }  # <tx.mn> [--corrupt] -> hex
validate_extrinsic() {  # <hex> -> TaggedTransactionQueue_validate_transaction at the best block (0x00... valid)
    local best; best=$(rpc chain_getBlockHash | jq -r '.result // empty')
    rpc state_call "[\"TaggedTransactionQueue_validate_transaction\",\"0x02$1${best#0x}\"]" | jq -r '.result // ("ERR:" + (.error.message // "?"))'
}
submit_extrinsic() {  # <hex> -> "accepted <hash>" or "<code> <message>: <data>"
    rpc author_submitExtrinsic "[\"0x$1\"]" | jq -r 'if .result then "accepted \(.result)" else "\(.error.code) \(.error.message): \(.error.data // "")" end' | head -1
}

# --- Indexer ------------------------------------------------------------------------
INDEXER_GQL_PATHS="${INDEXER_GQL_PATHS:-/api/v4/graphql /api/v3/graphql}"
indexer_probe() {  # <base-url> -> the first GraphQL endpoint that answers
    local p
    [ -n "$1" ] || return 1
    for p in $INDEXER_GQL_PATHS; do
        curl -s -m 15 -X POST "$1$p" -H 'Content-Type: application/json' -d '{"query":"{ block { height } }"}' 2>/dev/null \
            | jq -e '.data.block.height' >/dev/null 2>&1 && { echo "$1$p"; return 0; }
    done
    return 1
}
indexer_resolve() {  # [--probe: also when INDEXER_GQL is preset]; sets INDEXER_GQL and INDEXER_WS
    { [ "${1:-}" != --probe ] && [ -n "$INDEXER_GQL" ]; } || INDEXER_GQL=$(indexer_probe "$INDEXER_BASE") || return 1
    INDEXER_WS="$(echo "$INDEXER_GQL" | sed -e 's#^https://#wss://#' -e 's#^http://#ws://#')/ws"
}
gql() { curl -s -m 20 -X POST "$INDEXER_GQL" -H 'Content-Type: application/json' -d "$1"; }
gql_query() { gql "$(jq -cn --arg q "$1" '{query: $q}')"; }
indexer_height() { gql_query '{ block { height } }' | jq -r '.data.block.height // 0' 2>/dev/null || echo 0; }
indexer_block()  { gql_query '{ block { height hash protocolVersion dustGenerationEndIndex } }' | jq -c '.data.block'; }
indexer_contract_type() {  # <address> -> __typename of its latest action
    gql_query "{ contractAction(address: \"$1\") { __typename } }" | jq -r '.data.contractAction.__typename // empty'
}
indexer_catch_up() {  # [timeout-s]: the indexer reaches the finalized head
    local target waited=0 h
    target=$(finalized_height); [ -n "$target" ] || { echo "node unreachable"; return 1; }
    while [ "$waited" -lt "${1:-180}" ]; do
        h=$(indexer_height); [ "${h:-0}" -ge "$target" ] && return 0
        sleep 3; waited=$((waited + 3))
    done
    echo "indexer at ${h:-?}, node finalized $target"; return 1
}
indexer_serves_wallet_fields() {  # the progress fields wallet-sdk 2.x selects
    local t
    for t in ShieldedTransactionsProgress UnshieldedTransactionsProgress; do
        gql_query "{ __type(name: \"$t\") { fields { name } } }" \
            | jq -e '.data.__type.fields[]?.name | select(. == "protocolVersion")' >/dev/null 2>&1 \
            || { echo "$t has no protocolVersion field"; return 1; }
    done
}

state_tag() { printf '%s' "$1" | cut -c1-80 | xxd -r -p 2>/dev/null | LC_ALL=C tr -c '[:print:]' '.' | grep -o 'contract-state\[v[0-9]*\]' | head -1; }
# Node and indexer must serve the same state bytes, also for contracts untouched since the fork.
contract_state_node_vs_indexer() {  # <address> [evidence-prefix]
    local n i
    n=$(rpc midnight_contractState "[\"$1\"]" | jq -r '.result // empty')
    i=$(gql_query "{ contract(address: \"$1\") { state } }" | jq -r '.data.contract.state // empty')
    i="${i#0x}"; n="${n#0x}"
    [ -n "${2:-}" ] && { printf '%s' "$n" > "$2.node.hex"; printf '%s' "$i" > "$2.indexer.hex"; }
    echo "node $(state_tag "$n") $(( ${#n} / 2 ))B | indexer $(state_tag "$i") $(( ${#i} / 2 ))B"
    [ -n "$n" ] && [ "$n" = "$i" ]
}

contract_state_at() {  # <address> [block-hash] -> the serialised state, hex without 0x
    local r; r=$(rpc midnight_contractState "[\"$1\"${2:+,\"$2\"}]" | jq -r '.result // empty'); printf '%s' "${r#0x}"
}
# The data part of a contract's state, decoded with the ledger its tag names: "tag hash ops".
contract_data_at() {  # <address> <block-hash> <evidence-prefix>
    contract_state_at "$1" "$2" > "$3.hex"
    [ -s "$3.hex" ] || { echo "no state served at $2"; return 1; }
    contract_data "$3.hex"
}
# The fork re-encodes contract state; its data must come out unchanged.
contract_data_across_fork() {  # <address> <fork height> <evidence-prefix>
    local pre post
    pre=$(contract_data_at "$1" "$(block_hash_at "$NODE_HTTP" $(( $2 - 1 )))" "$3.fork_pre") || { echo "#$(( $2 - 1 )): $pre"; return 1; }
    post=$(contract_data_at "$1" "$(block_hash_at "$NODE_HTTP" "$2")" "$3.fork_post") || { echo "#$2: $post"; return 1; }
    echo "${pre% *} -> ${post% *}" | awk '{ printf "%s %.16s -> %s %.16s\n", $1, $2, $4, $5 }'
    [ "$(echo "$pre" | cut -d' ' -f2)" = "$(echo "$post" | cut -d' ' -f2)" ]
}

# midnight-indexer#1605: for a contract untouched since the fork the indexer serves its last
# pre-fork state. Known only when it serves exactly those bytes, with the data the node has.
indexer_1605() {  # <address> <evidence-prefix of contract_state_node_vs_indexer> <fork height>
    local node idx
    [ "$(state_tag "$(cat "$2.node.hex")")" = "contract-state[v8]" ] && [ "$(state_tag "$(cat "$2.indexer.hex")")" = "contract-state[v6]" ] || return 1
    contract_state_at "$1" "$(block_hash_at "$NODE_HTTP" $(( $3 - 1 )))" > "$2.fork_pre.hex"
    cmp -s "$2.fork_pre.hex" "$2.indexer.hex" || return 1
    node=$(contract_data "$2.node.hex") && idx=$(contract_data "$2.indexer.hex") || return 1
    [ "$(cut -d' ' -f2 <<< "$node")" = "$(cut -d' ' -f2 <<< "$idx")" ]
}

tx_hashes_in() {  # <logs...>; the toolkit colours its log fields, so colours go first
    cat "$@" 2>/dev/null | sed $'s/\x1b\\[[0-9;]*m//g' | grep -oE 'midnight_tx_hash="?(0x)?[0-9a-f]{64}' | grep -oE '[0-9a-f]{64}$' | sort -u
}
tx_inclusion_check() {  # <out.tsv> <logs...>: every tx hash in the logs is in a block
    local out=$1 h r height total=0 missing=0; shift
    printf 'tx_hash\tblock_height\n' > "$out"
    for h in $(tx_hashes_in "$@"); do
        total=$((total + 1))
        r=$(gql_query "{ transactions(offset: { hash: \"$h\" }) { block { height } } }")
        height=$(echo "$r" | jq -r '.data.transactions[0].block.height // empty')
        [ -n "$height" ] || { height=NOT_INDEXED; missing=$((missing + 1)); }
        printf '%s\t%s\n' "$h" "$height" >> "$out"
    done
    echo "$((total - missing))/$total submitted transactions found in blocks"
    [ "$total" -gt 0 ] && [ "$missing" = 0 ]
}

# --- Wallets and DUST ----------------------------------------------------------------
wallet_snapshot() {  # <seed> <out.json>
    tk_chain_ro -q show-wallet --seed "$1" 2>"$2.err" | sed -n '/^{/,$p' > "$2"
    jq -e '.utxos != null and .coins != null' "$2" >/dev/null 2>&1
}
wallet_night_unshielded() { jq -r --arg t "$NIGHT_TOKEN_TYPE" '[.utxos[] | select(.token_type == $t) | .value] | add // 0' "$1"; }
wallet_night_shielded()   { jq -r --arg t "$NIGHT_TOKEN_TYPE" '[.coins[]? | select(.token_type == $t) | .value] | add // 0' "$1"; }
wallet_night() { echo "$(wallet_night_unshielded "$1") $(wallet_night_shielded "$1")"; }  # <wallet.json> -> "unshielded shielded"
# NIGHT unchanged; on a network other wallets may send to ours, so there it must only not drop.
night_kept() {  # <before "unshielded shielded"> <after>
    [ -n "$1" ] && [ -n "$2" ] || return 1
    [ "$1" = "$2" ] && return 0
    [ "$TARGET" = network ] && ! big_gt "${1% *}" "${2% *}" && ! big_gt "${1#* }" "${2#* }"
}

dust_snapshot() {  # <seed> <out.json>: {"total": n, "source": {<backing NIGHT>: n}}
    tk_chain_ro -q dust-balance --seed "$1" 2>"$2.err" | sed -n '/^{/,$p' > "$2"
    jq -e '.total != null and .source != null' "$2" >/dev/null 2>&1
}
DUST_SOURCE_CAP="${DUST_SOURCE_CAP:-250000000000000000000000}"
# Judged per source below the cap: the wallet total hides that a spent UTxO's DUST decays
# while its change regenerates.
dust_growth() {  # <s1.json> <s2.json>
    python3 - "$1" "$2" "$DUST_SOURCE_CAP" <<'PY'
import json, sys
s1 = json.load(open(sys.argv[1]))["source"]; s2 = json.load(open(sys.argv[2]))["source"]; cap = int(sys.argv[3])
growing = [k for k in s1 if k in s2 and int(s2[k]) > int(s1[k])]
decaying = [k for k in s1 if k in s2 and int(s2[k]) < int(s1[k])]
capped = [k for k in s1 if int(s1[k]) >= cap]
print(f"{len(s1)} -> {len(s2)} sources; growing {len(growing)}, decaying {len(decaying)}, at cap {len(capped)}")
sys.exit(0 if growing or (s1 and len(capped) == len(s1)) else 1)
PY
}
dust_growth_check() {  # <seed> <file-prefix> [blocks]
    dust_snapshot "$1" "$2_s1.json" || { echo "dust-balance unreadable ($2_s1.json.err)"; return 1; }
    wait_blocks "${3:-8}" || return 1
    dust_snapshot "$1" "$2_s2.json" || { echo "dust-balance unreadable ($2_s2.json.err)"; return 1; }
    dust_growth "$2_s1.json" "$2_s2.json"
}

# --- Contracts ----------------------------------------------------------------------
# The serialised state changes across the fork by design, so contracts are compared by entry
# points and maintenance authority. Ledger 9 prefixes the authority key with a scheme byte,
# so only its last 64 hex characters count.
contract_semantic_state() {  # <address>
    local raw
    raw=$(tk_chain_ro contract-state --contract-address "$1" 2>&1) || { echo "QUERY_FAILED"; return 1; }
    {
        echo "$raw" | grep -oE '^Op: [a-zA-Z0-9_]+' | sed 's/^Op: /op=/' | sort
        echo "$raw" | grep -oE 'Authority VerifyingKey: [0-9a-f]+' | sed -E 's/.*: 0?0?([0-9a-f]{64})$/authority_key=\1/'
        echo "$raw" | grep -oE 'Authority Threshold: [0-9]+' | sed 's/.*: /authority_threshold=/'
        echo "$raw" | grep -oE 'Authority Counter: [0-9]+' | sed 's/.*: /authority_counter=/'
    }
}
semantic_snapshot() {  # <address> <out.txt>
    contract_semantic_state "$1" > "$2" && grep -q '^op=' "$2"
}

# Every contract the suite deployed, for the post-fork checks: utc label kind address era.
registry_add() {  # <label> <kind> <address> <era>
    [ -s "$REGISTRY" ] || printf 'utc\tlabel\tkind\taddress\tera\n' > "$REGISTRY"
    grep -q "	$3	" "$REGISTRY" && return 0
    printf '%s\t%s\t%s\t%s\t%s\n' "$(utc_now)" "$1" "$2" "$3" "$4" >> "$REGISTRY"
}
registry_addresses() {  # [era]
    [ -s "$REGISTRY" ] && tail -n +2 "$REGISTRY" | awk -F'\t' -v e="${1:-}" 'e == "" || $5 == e { print $4 }'
}
registry_label() { awk -F'\t' -v a="$1" '$4 == a { print $2; exit }' "$REGISTRY" 2>/dev/null; }

# key=value state shared between phases.
state_set() {  # <file> <key> <value>
    local f="$1" k="$2" v="$3"
    touch "$f"
    grep -v "^$k=" "$f" > "$f.tmp" 2>/dev/null || true
    printf '%s=%q\n' "$k" "$v" >> "$f.tmp"
    mv "$f.tmp" "$f"
}
state_load() { [ -s "$1" ] && source "$1"; return 0; }
