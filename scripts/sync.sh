#!/usr/bin/env bash

# This file is part of midnight-node.
# Copyright (C) 2025 Midnight Foundation
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

set -Eeuo pipefail


SYNC_UNTIL="${SYNC_UNTIL:-}"
if [ -n "$SYNC_UNTIL" ] && [[ ! "$SYNC_UNTIL" =~ ^[0-9]+$ ]]; then
    echo "SYNC_UNTIL must be a number"
    exit
fi

# Ensure CFG_PRESET is defined
if [[ -z "$CFG_PRESET" ]]; then
    echo "CFG_PRESET is not defined"
    exit
fi

# Ensure BOOTNODES is defined
if [[ -z "$BOOTNODES" ]]; then
    echo "BOOTNODES is not defined"
    exit
fi

if [[ -z "$NODE_IMAGE" ]]; then
    echo "Building container..."
    earthly +node-image
    NODE_IMAGE="ghcr.io/midnight-ntwrk/midnight-node:latest"
fi

BASE_PATH="${BASE_PATH:-}"
if [[ -n "$BASE_PATH" ]]; then
    BASE_PATH=$(realpath $BASE_PATH)
    BASE_PATH_ARGS=(-v "$BASE_PATH:/base_path" --env "BASE_PATH=/base_path")
    echo "${BASE_PATH_ARGS[@]}"
fi

context_name=$(kubectl config get-contexts -o name | grep teleport.prd.midnight.tools-k0-eks-tooling-stg-eu-01)
if [[ -z "$context_name" ]]; then
    echo "Error: could not find context matching name \"teleport.prd.midnight.tools-k0-eks-tooling-stg-eu-01\""
    echo "Check using \"kubectl config get-contexts\""
    exit 1
fi

NAMESPACE=${NAMESPACE:-qanet-spo-01}
APPEND_ARGS=${APPEND_ARGS:-}

DBSYNC_POD_NAME=${DBSYNC_POD_NAME:-}
if [[ -z "$DBSYNC_POD_NAME" ]]; then
    echo "finding db-sync pod name..."
    DBSYNC_POD_NAME="$(kubectl get pods -n qanet-spo-01 -o name --context "$context_name" | grep 'db-sync-cardano-01-0')"
    echo "found: $DBSYNC_POD_NAME"
fi

DBSYNC_POSTGRES_POD_NAME=${DBSYNC_POSTGRES_POD_NAME:-}
if [[ -z "$DBSYNC_POSTGRES_POD_NAME" ]]; then
    echo "finding db-sync postgres pod name..."
    DBSYNC_POSTGRES_POD_NAME="$(kubectl get pods -n    qanet-spo-01 -o name --context "$context_name" | grep 'psql-dbsync-cardano-01-db')"
    echo "found: $DBSYNC_POSTGRES_POD_NAME"
fi

# Setup trap to kill background processes on exit
trap "trap - SIGTERM && kill -- -$$" SIGINT SIGTERM EXIT

kubectl port-forward --address 0.0.0.0 -n $NAMESPACE \
  $DBSYNC_POSTGRES_POD_NAME \
  5432:5432 &

DBSYNC_ENV=$(kubectl exec $DBSYNC_POD_NAME -n $NAMESPACE --context "$context_name" -- env | cat)
export DB_SYNC_USER=$(env $DBSYNC_ENV bash -c 'echo $POSTGRES_USER')
export DB_SYNC_PASSWORD=$(env $DBSYNC_ENV bash -c 'echo $POSTGRES_PASSWORD')
export DB_SYNC_POSTGRES_CONNECTION_STRING="postgres://$DB_SYNC_USER:$DB_SYNC_PASSWORD@host.docker.internal:5432/cexplorer"

# Randomly generated 64 char node key
NODE_KEY=$(head -c 32 /dev/urandom | hexdump -v -e '/1 "%02x"')
node_temp_dir=$(mktemp -d 2>/dev/null || mktemp -d -t 'mnnode')
echo "$NODE_KEY" > "$node_temp_dir/node_key.txt"

TMPFILE=$(mktemp)

docker run \
    "${BASE_PATH_ARGS[@]}" \
    --env SHOW_CONFIG=true \
    --env DB_SYNC_POSTGRES_CONNECTION_STRING=$DB_SYNC_POSTGRES_CONNECTION_STRING \
    --env CFG_PRESET="$CFG_PRESET" \
    --env BOOTNODES="$BOOTNODES" \
    --env APPEND_ARGS="$APPEND_ARGS" \
    --env PGSSLMODE="disable" \
    -e NODE_KEY_FILE="/node_key.txt" \
    --add-host=host.docker.internal:host-gateway \
    -v "$node_temp_dir/node_key.txt:/node_key.txt" \
    $NODE_IMAGE 2>&1 | tee $TMPFILE &

# NOTE: The script can't keep up with node output if -l=debug is enabled

best_block=0
cur_best_block=0
last_block_increase_time=$(date +%s)

# Check if SYNC_UNTIL is set
if [[ ! -z "$SYNC_UNTIL" ]]; then
    tail -f "$TMPFILE" | while true; do
        read -t 20 -r line 
        if [ $? -ne 0 ]; then
            echo "No output from node in >20 seconds"
            echo "Total blocks synced: $best_block"
            echo "Exiting..."
            kill $(jobs -p)
            exit 1
        fi

        # Check if docker container is running
        if ! docker ps | grep -q $NODE_IMAGE; then
            echo "Container exited unexpectedly"
            echo "Exiting..."
            kill $(jobs -p)
            exit 1
        fi
        if echo "$line" | grep -q "best: "; then
            if [[ "$line" =~ best:\ \#([0-9]+) ]]; then
                number="${BASH_REMATCH[1]}"
                best_block=$number
                if [[ "$number" -ge "$SYNC_UNTIL" ]]; then
                    echo "Num blocks synced $number >= $SYNC_UNTIL"
                    echo "Exiting..."
                    kill $(jobs -p)
                    exit 0
                fi
            fi
        fi
        if [[ "$best_block" -gt "$cur_best_block" ]]; then
            cur_best_block=$best_block
            last_block_increase_time=$(date +%s)
        fi
        if [[ $(date +%s) -gt $((last_block_increase_time+20)) ]]; then
            echo "No sync progress in >20 seconds"
            echo "Total blocks synced: $best_block"
            echo "Exiting..."
            kill $(jobs -p)
            exit 1
        fi
    done
else
    while sleep 5; do
        # Check if docker container is running
        if ! docker ps | grep -q $NODE_IMAGE; then
            echo "Container exited unexpectedly"
            echo "Exiting..."
            kill $(jobs -p)
            exit 1
        fi
    done
fi

if [[ $? -ne 0 ]]; then
    rm $TMPFILE
    kill $(jobs -p)
    exit 1
else
    rm $TMPFILE
    kill $(jobs -p)
    exit 0
fi
