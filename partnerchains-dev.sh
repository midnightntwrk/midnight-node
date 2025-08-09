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

# Default configuration
push_image=false
local=false
mount_scripts=false
# Iterate through all arguments
for arg in "$@"; do
    if [[ "$arg" == "--push-image" ]]; then
        push_image=true
    fi

    if [[ "$arg" == "--local" ]]; then
        local=true
    fi

    if [[ "$arg" == "--mount-scripts" ]]; then
        mount_scripts=true
    fi
done

echo "Building partnerchains-dev container..."

# Check if --push-image flag is passed
if [[ "$push_image" == true ]]; then
    earthly --platform linux/amd64 --push +partnerchains-dev
else
    earthly +partnerchains-dev
fi

NODE_POD_NAME=${POD_NAME:-db-sync-cardano-node-01-0}
POSTGRES_POD_GREP=${POSTGRES_POD_GREP:-psql-dbsync-cardano-01-db}
DBSYNC_POD_NAME=${POD_NAME:-db-sync-cardano-01-0}
NAMESPACE=${NAMESPACE:-qanet}

# Check kubectl is installed
if ! command -v kubectl &> /dev/null
then
    echo "kubectl could not be found, please install it"
    exit
fi

context_name=$(kubectl config get-contexts -o name | grep k0-eks-platform-dev-eu-01)
if [[ -z "$context_name" ]]; then
    echo "Error: could not find context matching name \"k0-eks-platform-dev-eu-01\""
    echo "Check using \"kubectl config get-contexts\""
    exit 1
fi

postgres_pod_name=$(kubectl get pods -n qanet -o name | grep $POSTGRES_POD_GREP | sed 's/^pod\///')
if [[ -z "$postgres_pod_name" ]]; then
    echo "Error: could not find posgres pod matching name \"$POSTGRES_POD_GREP\""
    echo "Check using \"kubectl config get-contexts\""
    exit 1
fi

# Make a function for port forwarding
function port_forward_pod {
    POD_NAME=$1
    PORT=$2
    # Check cardano-node-01-0 pod is running
    if ! kubectl get pod "$POD_NAME" -n $NAMESPACE --context "$context_name"
    then
        echo "$POD_NAME pod is not running"
        exit
    fi

    kubectl port-forward --address 0.0.0.0 -n $NAMESPACE \
              --context "$context_name" \
              $POD_NAME \
              $PORT:$PORT &

}

trap "trap - SIGTERM && kill -- -$$" SIGINT SIGTERM EXIT

# Port forward node & socat to socket file
port_forward_pod $NODE_POD_NAME 30000
# Port forward dbsync
port_forward_pod $postgres_pod_name 5432

echo "Writing postgres connection string to pc-chain-config.json..."

DBSYNC_ENV=$(kubectl exec $DBSYNC_POD_NAME -n $NAMESPACE --context "$context_name" -- env | cat)
export DB_SYNC_POSTGRES_USER=$(env $DBSYNC_ENV bash -c 'echo $POSTGRES_USER')
export DB_SYNC_POSTGRES_PASSWORD=$(env $DBSYNC_ENV bash -c 'echo $POSTGRES_PASSWORD')

sleep 2

TMP_CONTAINER_NAME="partnerchains-dev-tmp"
CONTAINER_NAME="partnerchains-dev"
IMAGE_NAME="partnerchains-dev-local"

if docker ps -a --format "{{.Names}}" | grep -q "^$CONTAINER_NAME$"; then
    docker rm -f "$CONTAINER_NAME"
    echo "Removed old container: $CONTAINER_NAME"
fi

echo "Starting dev container..."

# Create a temporary container from the image
docker create --name $TMP_CONTAINER_NAME --platform linux/amd64 ghcr.io/midnight-ntwrk/partnerchains-dev:latest

# If local mode, replace partnerchains-dev scripts
if [[ "$local" == true ]]; then
    docker cp $PWD/scripts/partnerchains-dev/. "$TMP_CONTAINER_NAME:/"
fi

# Create a new image from the maybe modified container
docker commit $TMP_CONTAINER_NAME $IMAGE_NAME

# Remove temporary container
docker rm $TMP_CONTAINER_NAME

mount_scripts_args=()
if [ "${mount_scripts,,}" == "true" ]; then
    if [ -d "$PWD/scripts" ]; then
        echo "mounting scripts/ ..."
        mount_scripts_args=("-v" "$PWD/scripts:/scripts")
    fi
fi

# Run container with the new image
docker run -it \
    --env DB_SYNC_POSTGRES_USER=$DB_SYNC_POSTGRES_USER \
    --env DB_SYNC_POSTGRES_PASSWORD=$DB_SYNC_POSTGRES_PASSWORD \
    --add-host=host.docker.internal:host-gateway \
    --name $CONTAINER_NAME \
    -v $PWD/res:/res \
    ${mount_scripts_args[@]} \
    --platform linux/amd64 \
    $IMAGE_NAME
