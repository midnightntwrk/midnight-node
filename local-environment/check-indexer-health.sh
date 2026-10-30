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

# Fails if the local-env indexer stopped following the node: an indexer
# container exited, or chain-indexer doesn't reach the node's current
# finalized height. A node/indexer ledger divergence (e.g. PM-20886) shows up
# as chain-indexer exiting.

set -euo pipefail

timeout=300
node_url="http://localhost:9933"
graphql_url="http://localhost:8088/api/v3/graphql"
containers=(chain-indexer wallet-indexer indexer-api)

while [[ $# -gt 0 ]]; do
    case $1 in
        -t|--timeout) timeout="$2"; shift 2 ;;
        -u|--url) node_url="$2"; shift 2 ;;
        -g|--graphql-url) graphql_url="$2"; shift 2 ;;
        *)
            echo "Usage: $0 [-t|--timeout SECONDS] [-u|--url NODE_RPC_URL] [-g|--graphql-url INDEXER_GRAPHQL_URL]"
            exit 1
            ;;
    esac
done

rpc() {
    curl -s -X POST "$node_url" -H "Content-Type: application/json" -d "$1"
}

check_containers() {
    for c in "${containers[@]}"; do
        local state
        state=$(docker inspect -f '{{.State.Status}} exit={{.State.ExitCode}}' "$c" 2>/dev/null) || state="missing"
        if [[ "$state" != running* ]]; then
            echo "❌ $c is not running ($state)"
            docker logs --tail 80 "$c" 2>&1 || true
            exit 1
        fi
    done
}

check_containers

finalized_hash=$(rpc '{"jsonrpc":"2.0","id":1,"method":"chain_getFinalizedHead","params":[]}' | jq -r '.result')
target=$(( $(rpc "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"chain_getHeader\",\"params\":[\"$finalized_hash\"]}" | jq -r '.result.number') ))
echo "📡 Waiting for indexer to reach node finalized height $target (timeout: ${timeout}s)..."

start=$(date +%s)
height=""
while true; do
    check_containers
    height=$(curl -s --max-time 5 -X POST "$graphql_url" -H "Content-Type: application/json" \
        -d '{"query":"query { block { height } }"}' | jq -r '.data.block.height // empty' 2>/dev/null || true)
    if [[ -n "$height" && "$height" -ge "$target" ]]; then
        echo "✅ Indexer at height $height (node finalized $target)"
        exit 0
    fi
    if (( $(date +%s) - start >= timeout )); then
        echo "❌ Indexer at height ${height:-unknown} after ${timeout}s, node finalized $target"
        docker logs --tail 80 chain-indexer 2>&1 || true
        exit 1
    fi
    sleep 5
done
