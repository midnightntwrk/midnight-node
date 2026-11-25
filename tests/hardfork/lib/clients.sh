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
PROOF_SERVER_IMAGE="$IMAGE_REGISTRY/proof-server"
PROOF_SERVER_OLD_URL="${PROOF_SERVER_OLD_URL:-http://127.0.0.1:6300}"
PROOF_SERVER_NEW_URL="${PROOF_SERVER_NEW_URL:-http://127.0.0.1:6301}"

proof_server_version() { curl -s -m 5 "$1/version" 2>/dev/null; }

proof_server_up() {  # <old|new>: a remote one as it is; a local one only at the matrix version
    local era=$1 url tag name port v
    if [ "$era" = old ]; then url=$PROOF_SERVER_OLD_URL; tag=$PROOF_SERVER_OLD_TAG; else url=$PROOF_SERVER_NEW_URL; tag=$PROOF_SERVER_NEW_TAG; fi
    v=$(proof_server_version "$url")
    case "$url" in
        http://127.0.0.1:*|http://localhost:*) [ "$v" = "$tag" ] && return 0 ;;
        *) [ -n "$v" ] && return 0; echo "no proof server answering at $url" >&2; return 1 ;;
    esac
    port="${url##*:}"; name="hf-proof-server-$era-$port"   # per port: never replaces another stack's server
    # An earlier run's server at another version still holds the port.
    docker ps -a --filter "publish=$port" --format '{{.Names}}' | grep '^hf-proof-server-' | xargs docker rm -f >/dev/null 2>&1 || true
    docker rm -f "$name" >/dev/null 2>&1 || true
    docker run -d --name "$name" -p "$port:6300" "$PROOF_SERVER_IMAGE:$tag" >/dev/null || return 1
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

# The client scripts are the same for every fork (clients/). The fork pins the client
# releases they run against: its clients/ holds only package.json and the lock file, and
# the scripts find the installed packages through a link.
CLIENTS_DIR="$SUITE_DIR/clients"
CLIENT_PACKAGES_DIR="$FORK_DIR/clients"
# The wallet release from before the fork, for the wallet that never upgraded.
CLIENTS_LEGACY_DIR="$FORK_DIR/clients-legacy"

# npm ci again whenever package-lock.json differs from the one that was installed.
clients_ready() {  # [workspace dir]
    local d="${1:-$CLIENT_PACKAGES_DIR}" want
    want=$(sha256_of "$d/package-lock.json")
    if [ "$(cat "$d/node_modules/.hf-lock" 2>/dev/null)" != "$want" ]; then
        (cd "$d" && npm ci --no-audit --no-fund --loglevel=error) && echo "$want" > "$d/node_modules/.hf-lock" || return 1
    fi
    [ "$d" != "$CLIENT_PACKAGES_DIR" ] || ln -sfn "$CLIENT_PACKAGES_DIR/node_modules" "$CLIENTS_DIR/node_modules"
}

clients_versions() {  # as installed: "<name>=<version> ..." for CLIENT_PACKAGES of the matrix
    (cd "$CLIENT_PACKAGES_DIR" && node -e '
      const fs = require("node:fs");
      const v = (p) => { try { return JSON.parse(fs.readFileSync("node_modules/" + p + "/package.json", "utf8")).version } catch { return "absent" } };
      console.log(process.argv.slice(1).map((a) => a.split("=")).map(([n, p]) => n + "=" + v(p)).join(" "));' "${CLIENT_PACKAGES[@]}")
}
client_version() {  # <name> <clients_versions output>
    tr ' ' '\n' <<< "$2" | sed -n "s/^$1=//p"
}
clients_pinned() {  # <package>: the version clients/package.json pins
    jq -r --arg p "$1" '.dependencies[$p] // empty' "$CLIENT_PACKAGES_DIR/package.json"
}

# clients/ has no defaults: every endpoint is passed in.
client_env() {  # <seed> [new-ledger proof server url] [old-ledger proof server url]
    export MN_NETWORK_ID="$NETWORK_ID" MN_NODE_WS="$CLIENT_NODE_WS"
    export MN_INDEXER_HTTP="$INDEXER_GQL" MN_INDEXER_WS="$INDEXER_WS"
    export MN_OLD_LEDGER="$OLD_LEDGER" MN_NEW_LEDGER="$NEW_LEDGER"
    export MN_PROOF_SERVER_OLD="${3:-$PROOF_SERVER_OLD_URL}" MN_PROOF_SERVER_NEW="${2:-$PROOF_SERVER_NEW_URL}"
    export MN_PRE_FORK_SPEC="$OLD_EXPECTED_SPEC" MN_OUT="$EVIDENCE_DIR/clients" MN_SEED="$1"
    [ "$TARGET" = network ] && export MN_SYNC_TIMEOUT_MS="${MN_SYNC_TIMEOUT_MS:-900000}" MN_TIMEOUT_SECS="${MN_TIMEOUT_SECS:-3600}"
    mkdir -p "$MN_OUT"
}
# CLIENT_DIR=$CLIENTS_LEGACY_DIR runs the legacy workspace's script of that name.
client_run() {  # <out-prefix> <script> <args...>: stdout to <prefix>.json, stderr to <prefix>.err
    local prefix=$1 script=$2; shift 2
    ( client_env "${CLIENT_SEED:-$SENDER_SEED}" "${CLIENT_PROOF_SERVER_NEW:-}" "${CLIENT_PROOF_SERVER_OLD:-}"; cd "${CLIENT_DIR:-$CLIENTS_DIR}" && node "$script" "$@" ) \
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
    local out; out=$(cd "$CLIENTS_DIR" && MN_OLD_LEDGER="$OLD_LEDGER" MN_NEW_LEDGER="$NEW_LEDGER" node mjs_check.mjs state-data --hex-file "$1" 2>&1)
    jq -er '"\(.tag) \(.dataHash) \(.operations)"' <<< "$out" 2>/dev/null || { echo "undecodable: $(grep -m1 -o 'FAIL:.*' <<< "$out")"; return 1; }
}

chain_check() {  # <args...>
    ( cd "$CLIENTS_DIR" && MN_NODE_WS="$NODE_WS" node chain_check.mjs "$@" )
}

# --- Midnight.js ------------------------------------------------------------------------
# Midnight.js needs artefacts from the compiler that targets the compact-runtime it bundles.
# Artefacts resolve compact-runtime through a node_modules link to the fork's packages;
# artefacts for a contract deployed before the fork import it by its old-ledger alias.
midnightjs_compile() {  # <dApp> <artefacts dir name> <compactc version> [for-old-ledger]
    ln -sfn "$CLIENT_PACKAGES_DIR/node_modules" "$DAPPS_DIR/node_modules"
    DAPP_OUT="$2" DAPP_COMPACTC_VERSION="$3" dapp_compile "$1" "$OLD_TOOLKIT_IMAGE" || return 1
    [ "${4:-}" = for-old-ledger ] || return 0
    sed_i "s#@midnight-ntwrk/compact-runtime#$MIDNIGHTJS_OLD_RUNTIME_ALIAS#g" "$(dapp_dir "$1")/$2/contract/index.js"
}

# --- The wallet that never upgraded -----------------------------------------------------
# The wallet SDK release from before the fork (clients-legacy/). On the new ledger it may
# hang instead of failing, so it is stopped after a limit.
if [ "${TARGET:-local}" = local ]; then OLD_WALLET_LIMIT_SECS=120; else OLD_WALLET_LIMIT_SECS=600; fi
old_wallet_sdk_version() { jq -r .version "$CLIENTS_LEGACY_DIR/node_modules/@midnightntwrk/wallet-sdk/package.json"; }
old_wallet_send() {  # <out-prefix>: one unshielded transfer
    clients_ready "$CLIENTS_LEGACY_DIR" > "$1.install.log" 2>&1 || { echo "npm ci in $CLIENTS_LEGACY_DIR failed ($1.install.log)" > "$1.err"; return 2; }
    MN_SYNC_TIMEOUT_MS=$(( OLD_WALLET_LIMIT_SECS * 1000 )) MN_TIMEOUT_SECS=$OLD_WALLET_LIMIT_SECS \
        CLIENT_DIR="$CLIENTS_LEGACY_DIR" client_run "$1" wallet_check.mjs send --amount 1
}
