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

# Generates UTXO ordering override JSON files for each network by querying
# the indexer databases. These overrides allow syncing nodes to reproduce
# the HashMap-based UTXO ordering used before commit 91015433 switched
# to BTreeMap.
#
# Prerequisites:
#   - kubectl configured with access to the relevant clusters
#   - python3 with json module (stdlib)
#   - Correct kube contexts (see NETWORKS array below)
#
# Each network's indexer DB is accessed differently:
#   - qanet: CrunchyData PGO on dev cluster (kubectl exec into postgres pod)
#   - preview: RDS on preview cluster (ephemeral psql-query pod)
#   - preprod: RDS on preprod cluster (ephemeral psql-query pod)
#
# Usage:
#   ./scripts/generate-utxo-ordering-overrides.sh              # all networks
#   ./scripts/generate-utxo-ordering-overrides.sh preview      # single network
#   ./scripts/generate-utxo-ordering-overrides.sh qanet preprod # specific networks

set -Eeuo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
SQL_FILE="$SCRIPT_DIR/utxo-ordering.sql"
OUTPUT_DIR="$REPO_ROOT/res"

SQL="$(cat "$SQL_FILE")"

mkdir -p "$OUTPUT_DIR"

query_qanet() {
    local namespace="qanet"
    local secret="psql-indexer-rs-blue-pguser-indexer"
    local pod
    pod=$(kubectl get pods -n "$namespace" -l "postgres-operator.crunchydata.com/role=master,postgres-operator.crunchydata.com/cluster=psql-indexer-rs-blue" -o jsonpath='{.items[0].metadata.name}' 2>/dev/null || true)

    if [[ -z "$pod" ]]; then
        # Fallback: find any psql-indexer-rs-blue instance pod
        pod=$(kubectl get pods -n "$namespace" -o name 2>/dev/null | grep 'psql-indexer-rs-blue-instance' | head -1 | sed 's|pod/||')
    fi

    if [[ -z "$pod" ]]; then
        echo "ERROR: Could not find qanet postgres pod" >&2
        return 1
    fi

    local uri
    uri=$(kubectl get secret -n "$namespace" "$secret" -o jsonpath='{.data.uri}' | base64 -d)
    # Replace the service hostname with localhost since we exec into the pod
    local local_uri
    local_uri=$(echo "$uri" | sed 's|@[^/]*|@localhost:5432|')

    echo "  Pod: $pod" >&2
    echo "$SQL" | kubectl exec -i -n "$namespace" "$pod" -c database -- psql "$local_uri" -t -A -f - 2>/dev/null
}

query_rds() {
    local context="$1"
    local namespace="$2"
    local secret="rds-connection-details-indexer"

    local endpoint password port username
    endpoint=$(kubectl --context "$context" get secret -n "$namespace" "$secret" -o jsonpath='{.data.endpoint}' | base64 -d)
    password=$(kubectl --context "$context" get secret -n "$namespace" "$secret" -o jsonpath='{.data.password}' | base64 -d)
    port=$(kubectl --context "$context" get secret -n "$namespace" "$secret" -o jsonpath='{.data.port}' | base64 -d)
    username=$(kubectl --context "$context" get secret -n "$namespace" "$secret" -o jsonpath='{.data.username}' | base64 -d)

    local uri="postgresql://${username}:${password}@${endpoint}:${port}/indexer"

    echo "  RDS: $endpoint" >&2
    # Run an ephemeral pod on the target cluster to reach the RDS endpoint
    kubectl --context "$context" run psql-utxo-override-query \
        --rm -i --restart=Never -n "$namespace" \
        --image=postgres:15-alpine \
        -- psql "$uri" -t -A -c "$SQL" 2>/dev/null
}

extract_json() {
    # The raw output may contain kubectl messages around the JSON array.
    # Extract the outermost [...] JSON array.
    python3 -c "
import sys, json
raw = sys.stdin.read()
start = raw.find('[')
end = raw.rfind(']')
if start == -1 or end == -1:
    json.dump([], sys.stdout, indent=2)
    print()
    sys.exit(0)
data = json.loads(raw[start:end+1])
json.dump(data, sys.stdout, indent=2)
print()
"
}

run_network() {
    local network="$1"
    local outfile="$OUTPUT_DIR/utxo-ordering-override-${network}.json"

    echo "==> $network" >&2
    local raw
    case "$network" in
        qanet)
            raw=$(query_qanet)
            ;;
        preview)
            raw=$(query_rds "eks-euw1-preview-1" "preview")
            ;;
        preprod)
            raw=$(query_rds "stl-euw1-preprod-1" "preprod")
            ;;
        *)
            echo "ERROR: Unknown network: $network" >&2
            echo "  Supported: qanet, preview, preprod" >&2
            return 1
            ;;
    esac

    echo "$raw" | extract_json > "$outfile"
    local count
    count=$(python3 -c "import json; print(len(json.load(open('$outfile'))))")
    echo "  Wrote $count entries to $outfile" >&2
}

# Default: all networks
NETWORKS=("qanet" "preview" "preprod")
if [[ $# -gt 0 ]]; then
    NETWORKS=("$@")
fi

for network in "${NETWORKS[@]}"; do
    run_network "$network"
done

echo "" >&2
echo "Done." >&2
