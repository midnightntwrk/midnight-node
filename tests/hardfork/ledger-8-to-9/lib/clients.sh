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

# Proof servers, the wallet SDK and Midnight.js: the path users take, which the toolkit
# (replaying and proving in-process) never exercises.

# Proof servers are stateless, so a local container serves a deployed network too.
PS_IMAGE_REPO="${PS_IMAGE_REPO:-$IMAGE_REGISTRY/proof-server}"
PS_L8_URL="${PROOF_SERVER_L8:-http://127.0.0.1:${PS_L8_PORT:-6300}}"
PS_L9_URL="${PROOF_SERVER_L9:-http://127.0.0.1:${PS_L9_PORT:-6301}}"

proof_server_version() { curl -s -m 5 "$1/version" 2>/dev/null; }

proof_server_up() {  # <l8|l9>: reuse a running one, else start a local container
    local era=$1 url tag name port
    if [ "$era" = l8 ]; then url=$PS_L8_URL; tag=$PS_L8_TAG; else url=$PS_L9_URL; tag=$PS_L9_TAG; fi
    [ -n "$(proof_server_version "$url")" ] && return 0
    case "$url" in http://127.0.0.1:*|http://localhost:*) ;; *) echo "no proof server answering at $url" >&2; return 1 ;; esac
    name="hf-proof-server-$era"; port="${url##*:}"
    docker rm -f "$name" >/dev/null 2>&1 || true
    docker run -d --name "$name" -p "$port:6300" "$PS_IMAGE_REPO:$tag" >/dev/null || return 1
    for _ in $(seq 1 30); do
        [ -n "$(proof_server_version "$url")" ] && return 0
        sleep 1
    done
    echo "proof server $tag did not answer at $url" >&2
    return 1
}
proof_server_family_ok() {  # <url> <major>
    case "$(proof_server_version "$1")" in "$2".*) return 0 ;; *) return 1 ;; esac
}

CLIENTS_DIR="$SUITE_DIR/clients"

# npm ci again whenever package-lock.json changed since the last install.
clients_ready() {
    [ "$CLIENTS_DIR/node_modules/.package-lock.json" -nt "$CLIENTS_DIR/package-lock.json" ] && return 0
    (cd "$CLIENTS_DIR" && npm ci --no-audit --no-fund --loglevel=error)
}

clients_versions() {  # as installed
    (cd "$CLIENTS_DIR" && node -e '
      const fs = require("node:fs");
      const v = (p) => { try { return JSON.parse(fs.readFileSync("node_modules/" + p + "/package.json", "utf8")).version } catch { return "absent" } };
      console.log([
        "wallet-sdk=" + v("@midnightntwrk/wallet-sdk"),
        "midnight-js=" + v("@midnight-ntwrk/midnight-js-contracts"),
        "ledger-v8=" + v("@midnight-ntwrk/ledger-v8"),
        "ledger-v9=" + v("@midnightntwrk/ledger-v9"),
        "compact-js=" + v("@midnight-ntwrk/compact-js"),
        "compact-runtime=" + v("@midnight-ntwrk/compact-runtime"),
      ].join(" "));')
}
client_version() {  # <name> <clients_versions output>
    tr ' ' '\n' <<< "$2" | sed -n "s/^$1=//p"
}
clients_pinned() {  # <package>: the version clients/package.json pins
    jq -r --arg p "$1" '.dependencies[$p] // empty' "$CLIENTS_DIR/package.json"
}

# clients/ has no defaults: every endpoint is passed in.
client_env() {  # <seed> [ledger-9 proof server url]
    export MN_NETWORK_ID="$NETWORK_ID" MN_NODE_WS="$CLIENT_NODE_WS"
    export MN_INDEXER_HTTP="$INDEXER_GQL" MN_INDEXER_WS="$INDEXER_WS"
    export MN_PROOF_SERVER_V8="$PS_L8_URL" MN_PROOF_SERVER_V9="${2:-$PS_L9_URL}"
    export MN_PRE_FORK_SPEC="$L8_EXPECTED_SPEC" MN_OUT="$EVIDENCE_DIR/clients" MN_SEED="$1"
    [ "$TARGET" = network ] && export MN_SYNC_TIMEOUT_MS="${MN_SYNC_TIMEOUT_MS:-900000}" MN_TIMEOUT_SECS="${MN_TIMEOUT_SECS:-3600}"
    mkdir -p "$MN_OUT"
}
client_run() {  # <out-prefix> <script> <args...>: stdout to <prefix>.json, stderr to <prefix>.err
    local prefix=$1 script=$2; shift 2
    ( client_env "${CLIENT_SEED:-$SEED_1}" "${CLIENT_PS_V9:-}"; cd "$CLIENTS_DIR" && node "$script" "$@" ) \
        > "$prefix.json" 2> "$prefix.err"
}
client_error() {  # <out-prefix>: the client's FAIL line, else the last line of its stderr
    local e; e=$(grep -m1 'FAIL:' "$1.err" 2>/dev/null)
    printf '%s' "${e:-$(tail -1 "$1.err" 2>/dev/null)}" | cut -c1-220
}
# The node's custom error code for a transaction it refused, e.g. 170 for InvalidDustSpendProof.
client_custom_error() { jq -r '.error.customError // empty' "$1.json" 2>/dev/null; }

contract_data() {  # <state hex file> -> "tag data-hash operations"
    clients_ready > /dev/null 2>&1 || { echo "npm ci in clients/ failed"; return 1; }
    local out; out=$(cd "$CLIENTS_DIR" && node mjs_check.mjs state-data --hex-file "$1" 2>&1)
    jq -er '"\(.tag) \(.dataHash) \(.operations)"' <<< "$out" 2>/dev/null || { echo "undecodable: $(grep -m1 -o 'FAIL:.*' <<< "$out")"; return 1; }
}

chain_check() {  # <args...>
    ( cd "$CLIENTS_DIR" && MN_NODE_WS="$NODE_WS" node chain_check.mjs "$@" )
}
