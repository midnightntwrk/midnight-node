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

# Endpoints, seeds and toolkit of the target: TARGET=local (local-env), or TARGET=network
# with NETWORK_ENV and SEEDS_FILE.

source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

TARGET="${TARGET:-local}"

case "$TARGET" in
local)
    NETWORK_NAME="local"
    NETWORK_ID="undeployed"
    NODE_WS="ws://127.0.0.1:9933"
    NODE_HTTP="http://127.0.0.1:9933"
    NODES=(
        "midnight-node-1=http://127.0.0.1:9933"
        "midnight-node-2=http://127.0.0.1:9934"
        "midnight-node-3=http://127.0.0.1:9935"
        "midnight-node-4=http://127.0.0.1:9936"
        "midnight-node-5=http://127.0.0.1:9944"
    )
    CLIENT_NODE_WS="ws://127.0.0.1:9944"
    INDEXER_BASE="http://localhost:8088"
    INDEXER_GQL="$INDEXER_BASE/api/v4/graphql"
    # Public, genesis-funded seeds of the local-env chain.
    SEED_1="0000000000000000000000000000000000000000000000000000000000000001"
    SEED_2="0000000000000000000000000000000000000000000000000000000000000002"
    SEED_3="0000000000000000000000000000000000000000000000000000000000000003"
    SEED_4="0000000000000000000000000000000000000000000000000000000000000004"
    GOV_COUNCIL_URIS="${GOV_COUNCIL_URIS:-//Four,//Five}"
    GOV_TC_URIS="${GOV_TC_URIS:-//One,//Two}"
    GOV_EXECUTOR_URI="${GOV_EXECUTOR_URI:-//One}"
    SAFE_MODE_DRILL="${SAFE_MODE_DRILL:-1}"
    TOOLKIT_CACHE="${TOOLKIT_CACHE:-0}"
    ;;
network)
    [ -n "${NETWORK_ENV:-}" ] && [ -s "$NETWORK_ENV" ] \
        || { echo "TARGET=network needs NETWORK_ENV=<network/env/*.env>" >&2; exit 2; }
    l8_ref_default="$L8_REF"
    # shellcheck source=/dev/null
    source "$NETWORK_ENV"
    [ "$L8_REF" = "$l8_ref_default" ] || set_l8_images
    for v in NETWORK_NAME NETWORK_ID NODE_WS NODE_HTTP INDEXER_URL; do
        [ -n "${!v:-}" ] || { echo "$v missing in $NETWORK_ENV" >&2; exit 2; }
    done
    NODES=("rpc=$NODE_HTTP")
    for n in ${EXTRA_RPC_NODES:-}; do NODES+=("$n"); done
    CLIENT_NODE_WS="$NODE_WS"
    INDEXER_BASE="$INDEXER_URL"
    INDEXER_GQL=""   # set by indexer_resolve
    [ -n "${SEEDS_FILE:-}" ] && [ -s "$SEEDS_FILE" ] \
        || { echo "TARGET=network needs SEEDS_FILE=<file with SEED_1..SEED_4> (see network/seeds.env.example)" >&2; exit 2; }
    [ -z "$(find "$SEEDS_FILE" \( -perm -040 -o -perm -004 \) 2>/dev/null)" ] \
        || { echo "$SEEDS_FILE is readable by group or others: chmod 600 it" >&2; exit 2; }
    # shellcheck source=/dev/null
    source "$SEEDS_FILE"
    for i in 1 2 3 4; do
        v="SEED_$i"
        is_hex64 "${!v:-}" || { echo "$SEEDS_FILE: $v must be 64 lowercase hex characters" >&2; exit 2; }
    done
    SAFE_MODE_DRILL="${SAFE_MODE_DRILL:-0}"
    TOOLKIT_CACHE="${TOOLKIT_CACHE:-1}"
    ;;
*) echo "TARGET must be local or network, not '$TARGET'" >&2; exit 2 ;;
esac

OUT_DIR="${OUT_DIR:-$(out_dir_for "$TARGET" "$NETWORK_NAME")}"
init_out_dirs
NIGHT_TOKEN_TYPE="0000000000000000000000000000000000000000000000000000000000000000"
CNIGHT_SEED_INDEX="${CNIGHT_SEED_INDEX:-}"

# --- Toolkit ------------------------------------------------------------------------
# TOOLKIT_L8_BIN / TOOLKIT_L9_BIN select a native build instead of the image. On a network
# each era keeps a fetch and a ledger-state cache, so a long chain is replayed once. The
# caches are single-writer: never run toolkit commands concurrently.
FETCH_CONCURRENCY="${FETCH_CONCURRENCY:-8}"
FETCH_COMPUTE_CONCURRENCY="${FETCH_COMPUTE_CONCURRENCY:-4}"
export MN_REPLAY_CONCURRENCY="${MN_REPLAY_CONCURRENCY:-4}"
export MIDNIGHT_PP="${MIDNIGHT_PP:-$HOME/.cache/midnight/zk-params}"

use_toolkit() {  # <l8|l9>
    local docker_bin="$SUITE_DIR/lib/toolkit_docker.sh"
    case "$1" in
        l8) TK_ERA=l8; TK_IMAGE="$L8_TOOLKIT_IMAGE"; TK_BIN="${TOOLKIT_L8_BIN:-$docker_bin}" ;;
        l9) TK_ERA=l9; TK_IMAGE="$(l9_toolkit_image)"; TK_BIN="${TOOLKIT_L9_BIN:-$docker_bin}" ;;
        *) echo "use_toolkit: l8 or l9" >&2; return 1 ;;
    esac
    if [ "$TOOLKIT_CACHE" = 1 ]; then TK_CACHE_DIR="$CACHE_DIR/toolkit-$TK_ERA"; mkdir -p "$TK_CACHE_DIR"
    else TK_CACHE_DIR=""; fi
    export TOOLKIT_IMAGE="$TK_IMAGE" TOOLKIT_MOUNTS="$OUT_DIR ${TK_CACHE_DIR:-}"
}

tk() { "$TK_BIN" "$@"; }

tk_chain() {  # <toolkit args...>, against the target node
    local extra=()
    if [ -n "$TK_CACHE_DIR" ]; then
        extra+=(--fetch-cache "redb:$TK_CACHE_DIR/fetch_cache.db" --ledger-state-db "$TK_CACHE_DIR/ledger_cache_db"
                --fetch-concurrency "$FETCH_CONCURRENCY" --fetch-compute-concurrency "$FETCH_COMPUTE_CONCURRENCY")
    fi
    # Only the proving commands accept --proof-server.
    if [ -n "${PROOF_SERVER:-}" ]; then
        case "$1" in generate-txs|send-intent) extra+=(--proof-server "$PROOF_SERVER") ;; esac
    fi
    "$TK_BIN" "$@" -s "$NODE_WS" "${extra[@]}"
}

# Read-only replays are retried, since a public RPC drops long websocket connections; a
# failed attempt's output goes to stderr so callers parse only the last one. Submissions
# are not retried: a retry after a lost confirmation would submit twice.
tk_chain_ro() {
    local a out; out=$(mktemp)
    for a in 1 2 3; do
        tk_chain "$@" > "$out" && { cat "$out"; rm -f "$out"; return 0; }
        [ "$a" -lt 3 ] || break
        cat "$out" >&2; echo "[tk_chain_ro] attempt $a failed, retrying" >&2; sleep 5
    done
    cat "$out"; rm -f "$out"; return 1
}

# Toolkit containers carry the run's label, so an interrupted run can stop them.
export HF_TK_OWNER="${HF_TK_OWNER:-$$}"
_tk_kill_all() {
    docker ps -q --filter "label=hf-tk-owner=$HF_TK_OWNER" 2>/dev/null | xargs docker kill > /dev/null 2>&1 || true
}
trap '_tk_kill_all; exit 130' INT
trap '_tk_kill_all; exit 143' TERM

_kill_tree() {  # <pid>
    local c; for c in $(pgrep -P "$1"); do _kill_tree "$c"; done
    kill "$1" 2>/dev/null || true
}

# The toolkit is stopped on timeout, not just abandoned: an orphan could still submit and
# would share the single-writer caches with the next command.
TX_WATCHDOG_SECS="${TX_WATCHDOG_SECS:-900}"
_watchdog() {  # <seconds> <command...>; 124 on timeout
    local limit="$1" name="hf-tk-$HF_TK_OWNER-$$-$RANDOM" waited=0 pid; shift
    ( export TOOLKIT_CONTAINER_NAME="$name"; "$@" ) &
    pid=$!
    while kill -0 "$pid" 2>/dev/null; do
        if [ "$waited" -ge "$limit" ]; then
            echo "[watchdog] still running after ${limit}s, stopped (check inclusion on chain)" >&2
            docker kill "$name" > /dev/null 2>&1 || true
            _kill_tree "$pid"; wait "$pid" 2>/dev/null; return 124
        fi
        sleep 2; waited=$((waited + 2))
    done
    wait "$pid"
}
tk_tx()   { _watchdog "$TX_WATCHDOG_SECS" tk_chain "$@"; }
tk_send() { _watchdog "$TX_WATCHDOG_SECS" tk generate-txs --src-file "$1" -r 1 send -d "$NODE_WS"; }

tk_address() {  # <seed> <--unshielded|--shielded|--coin-public|...>
    tk show-address --network "$NETWORK_ID" --seed "$1" "$2" 2>/dev/null | tail -1
}

tk_version() { tk version 2>&1 | tr '\n' ' ' | sed 's/  */ /g'; }

use_toolkit l8

# --- Node logs ----------------------------------------------------------------------
# Local: the validator containers. Network: pods through kubectl when KUBE_CONTEXT is set.
_kubectl() { kubectl --context "$KUBE_CONTEXT" -n "${KUBE_NAMESPACE:-$NETWORK_NAME}" "$@"; }
logs_available() {
    [ "$TARGET" = local ] && return 0
    [ -n "${KUBE_CONTEXT:-}" ] && _kubectl --request-timeout=15s get pods >/dev/null 2>&1
}
log_nodes() {
    if [ "$TARGET" = local ]; then
        local n; for n in "${NODES[@]}"; do echo "${n%%=*}"; done
    else
        _kubectl --request-timeout=15s get pods -o jsonpath='{range .items[*]}{.metadata.name}{"\n"}{end}' 2>/dev/null \
            | grep -E "${KUBE_POD_REGEX:-^midnight-[0-9]+-}"
    fi
}
node_logs() {  # <node>
    if [ "$TARGET" = local ]; then docker logs "$1" 2>&1
    else _kubectl --request-timeout=60s logs "$1" --tail "${KUBE_LOG_TAIL:-200000}" 2>&1; fi
}
