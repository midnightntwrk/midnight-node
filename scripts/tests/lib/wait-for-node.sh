# shellcheck shell=bash
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

# Two flavours of "is the node ready" wait, picked deliberately at the call site:
#
#   wait_for_unfinalized_block <rpc_url> <target> [timeout]
#       Polls chain_getHeader (best chain head). Fast. Use when the test only
#       needs "node is alive and producing blocks". Best ≠ finalized: a best
#       block can still be reorged.
#
#   wait_for_finalized_block <rpc_url> <target> [timeout]
#       Polls chain_getFinalizedHead → chain_getHeader(hash) (GRANDPA-confirmed).
#       Slower but stable. Use when the test asserts something only meaningful
#       after finality (e.g. startup smoke check that finality is advancing).

# --- helpers --------------------------------------------------------------

# _rpc_get_best_height <url>: prints best block height as decimal, or empty on error.
_rpc_get_best_height() {
    local url="$1"
    local hex
    hex=$(curl -sf --max-time 2 -H 'Content-Type: application/json' \
        -d '{"jsonrpc":"2.0","method":"chain_getHeader","params":[],"id":1}' \
        "$url" 2>/dev/null \
        | grep -oE '"number"[[:space:]]*:[[:space:]]*"0x[0-9a-fA-F]+"' \
        | grep -oE '0x[0-9a-fA-F]+' || true)
    if [ -n "$hex" ]; then echo "$((hex))"; fi
}

# _rpc_get_finalized_height <url>: prints finalized block height as decimal, or empty on error.
_rpc_get_finalized_height() {
    local url="$1"
    local hash
    hash=$(curl -sf --max-time 2 -H 'Content-Type: application/json' \
        -d '{"jsonrpc":"2.0","method":"chain_getFinalizedHead","params":[],"id":1}' \
        "$url" 2>/dev/null \
        | grep -oE '"result"[[:space:]]*:[[:space:]]*"0x[0-9a-fA-F]+"' \
        | grep -oE '0x[0-9a-fA-F]+' || true)
    [ -z "$hash" ] && return
    local hex
    hex=$(curl -sf --max-time 2 -H 'Content-Type: application/json' \
        -d "{\"jsonrpc\":\"2.0\",\"method\":\"chain_getHeader\",\"params\":[\"${hash}\"],\"id\":1}" \
        "$url" 2>/dev/null \
        | grep -oE '"number"[[:space:]]*:[[:space:]]*"0x[0-9a-fA-F]+"' \
        | grep -oE '0x[0-9a-fA-F]+' || true)
    if [ -n "$hex" ]; then echo "$((hex))"; fi
}

# _wait_for_block_inner <fetcher_fn> <label> <url> <target> <timeout>
_wait_for_block_inner() {
    local fetcher="$1"
    local label="$2"
    local url="$3"
    local target="$4"
    local timeout="$5"
    local elapsed=0
    local last_height=""
    echo "⏳ Waiting up to ${timeout}s for ${label} block ≥ ${target} at ${url}..."
    while [ "$elapsed" -lt "$timeout" ]; do
        local height
        height=$("$fetcher" "$url")
        if [ -n "$height" ]; then
            last_height="$height"
            if [ "$height" -ge "$target" ]; then
                echo "✅ ${label} block ${height} ≥ ${target} after ${elapsed}s"
                return 0
            fi
        fi
        sleep 1
        elapsed=$((elapsed + 1))
    done
    if [ -n "$last_height" ]; then
        echo "❌ ${label} block ${target} not reached within ${timeout}s (last seen: ${last_height})"
    else
        echo "❌ ${label} block ${target} not reached within ${timeout}s (rpc unreachable or empty)"
    fi
    return 1
}

# --- public API -----------------------------------------------------------

# wait_for_unfinalized_block <rpc_url> <target_block> [timeout_secs]
wait_for_unfinalized_block() {
    _wait_for_block_inner _rpc_get_best_height "best" "$1" "$2" "${3:-90}"
}

# wait_for_finalized_block <rpc_url> <target_block> [timeout_secs]
wait_for_finalized_block() {
    _wait_for_block_inner _rpc_get_finalized_height "finalized" "$1" "$2" "${3:-90}"
}
