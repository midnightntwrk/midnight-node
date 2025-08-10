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

# Check if CURRENT_IMAGE is set
if [ -z "$CURRENT_IMAGE" ]; then
    echo "Warning: CURRENT_IMAGE is not set. Set this to the full image identifier representing the version of the network to upgrade FROM. E.g. 'ghcr.io/midnight-ntwrk/midnight-node:0.2.6-7e706b8'"
    exit 1;
else
    echo "Using provided CURRENT_IMAGE: $CURRENT_IMAGE"
fi
# Check if NEW_IMAGE is set
if [ -z "$NEW_IMAGE" ]; then
    echo "Warning: NEW_IMAGE is not set. Set this to the full image identifier representing the version of the network to upgrade TO. E.g. 'ghcr.io/midnight-ntwrk/midnight-node:0.2.6-7e706b8'"
    exit 1;
else
    echo "Using provided NEW_IMAGE: $NEW_IMAGE"
fi
# if [ -z "$CHAIN_SPEC" ]; then
#     echo "Warning: CHAIN_SPEC is not set. Set this to a valid path of a raw json chain spec(built with --raw option)"
#     exit 1;
# fi
if [ -z "$NODE_URL" ]; then
    echo "Warning: NODE_URL is not set. Set this to the full wss url of the given environment. E.g. 'wss://rpc.devnet.midnight.network'"
    exit 1;
fi

if [ -z "$CFG_PRESET" ]; then
    echo "Warning: CFG_PRESET is not set. Set the CFG_PRESET according to the network(local, devnet, qanet, testnet-02)"
    exit 1;
fi

if [ -z "$MOCK_FILE_NAME" ]; then
    echo "Warning: MOCK_FILE_NAME is not set. Set the file name of the mock data to use."
    exit 1;
fi

# Base path for the local network data
BASE_DATA_PATH="./data"

# Define the container names and the new image
BASE_CONTAINER_NAME="substrate-node"
: ${BASE_P2P_PORT:=30333}
: ${BASE_RPC_PORT:=9944}
: ${WASM_PATH:="../target/release/wbuild/midnight-node-runtime/midnight_node_runtime.compact.compressed.wasm"}

# Add any seed phrases in here(for now)
SEED_PHRASES=(
  "//Alice"
)

export_and_fork_state() {
    echo "Skipping fork step... TODO: fix this to work with large ledger state"
    # echo "Trying to fork state via node $NODE_URL"
    # if ! command -v subalfred &> /dev/null; then
    #     echo "Subalfred is not installed. Installing now..."
    #     cargo install subalfred
    # fi
    # subalfred state export "$NODE_URL" --skip-pallets "Grandpa,System,Aura,Timestamp"
    # subalfred state fork-off default-chain-spec.json.export --renew-consensus-with "$CHAIN_SPEC" --disable-default-bootnodes
    # mkdir ./data
    # cp default-chain-spec.json.export.fork-off ./data
}

# Function to stop a container
stop_container() {
    echo "Stopping container $1"
    docker stop $1
}

# Function to remove a container
remove_container() {
    echo "Removing container $1"
    docker rm $1
}

start_container() {
    local container_name=$1
    local image=$2
    local seed_phrase=$3
    local p2p_port=$4
    local rpc_host_port=$5
    local node_key=$6

    local container_data_path="${BASE_DATA_PATH}/${container_name}"
    mkdir -p "${container_data_path}"

    echo "$seed_phrase" > "$container_data_path/seed.secret"
    echo "$node_key" > "$container_data_path/node_key.secret"

    docker run  --network="host" -d --name "${container_name}" \
        -v "${container_data_path}:/data" \
        -p "${rpc_host_port}:9944" \
        -e AURA_SEED_FILE="/data/seed.secret" \
        -e GRANDPA_SEED_FILE="/data/seed.secret" \
        -e CROSS_CHAIN_SEED_FILE="/data/seed.secret" \
        -e BASE_PATH="/data" \
        -e NODE_KEY_FILE="/data/node_key.secret" \
        -e ARGS="--validator --rpc-port 9944 --port=${p2p_port} --unsafe-rpc-external" \
        -e DB_SYNC_POSTGRES_CONNECTION_STRING="postgres://host.docker.internal:5432" \
        -e USE_MAIN_CHAIN_FOLLOWER_MOCK="true" \
        -e MOCK_REGISTRATIONS_FILE="/res/mock-bridge-data/${MOCK_FILE_NAME}" \
        -e CFG_PRESET="${CFG_PRESET}" \
        "${image}"
}

start_local_network() {
    # Start initial network
    for i in "${!SEED_PHRASES[@]}"; do
        CONTAINER_NAME="${BASE_CONTAINER_NAME}$((i + 1))"
        P2P_PORT=$((BASE_P2P_PORT + i))
        RPC_HOST_PORT=$((BASE_RPC_PORT + i))
        NODE_KEY=$(printf "%064x" $((base_node_key + i)))
        echo "Starting node ${CONTAINER_NAME} with image ${CURRENT_IMAGE}, and P2P port: ${P2P_PORT}"
        # TODO: call subalfred to fork existing state and export to chain spec file
        start_container "${CONTAINER_NAME}" "${CURRENT_IMAGE}" "${SEED_PHRASES[i]}" "${P2P_PORT}" "${RPC_HOST_PORT}" "${NODE_KEY}"
    done
    echo "Local network started."
}
client_upgrade() {
    echo "Will begin swapping all nodes of image $CURRENT_IMAGE with $NEW_IMAGE"
    # Upgrade network by swapping runnning CURRENT_IMAGEs with NEW_IMAGEs
    for i in "${!SEED_PHRASES[@]}"; do
        CONTAINER_NAME="${BASE_CONTAINER_NAME}$((i + 1))"
        # Stop and remove the existing container
        stop_container "${CONTAINER_NAME}"
        remove_container "${CONTAINER_NAME}"
        echo "Restarting node ${CONTAINER_NAME} with image ${CURRENT_IMAGE}, and P2P port: ${P2P_PORT}"
        P2P_PORT=$((BASE_P2P_PORT + i))
        RPC_HOST_PORT=$((BASE_RPC_PORT + i))  
        NODE_KEY=$(printf "%064x" $((base_node_key + i)))
        start_container "${CONTAINER_NAME}" "${NEW_IMAGE}" "${SEED_PHRASES[i]}" "${P2P_PORT}" "${RPC_HOST_PORT}" "${NODE_KEY}"
        sleep 1;
    done
    echo "Client upgrade completed."
}
stop_network() {
    for i in "${!SEED_PHRASES[@]}"; do
        CONTAINER_NAME="${BASE_CONTAINER_NAME}$((i + 1))"
        stop_container "${CONTAINER_NAME}"
        remove_container "${CONTAINER_NAME}"
    done
}
runtime_upgrade() {
    echo "Preparing for runtime upgrade by retrieving runtime metadata from node"
    # 1. Ensure you have subxt-cli, then we run this to retrieve the current version of the runtime's metadata
    subxt metadata -f bytes > mn-metadata.scale
    mv mn-metadata.scale ../res/subxt/
    cargo run -p upgrader -- -t 0 --runtime-path $WASM_PATH
}
# Function to display usage
usage() {
    echo "Usage: $0 [option]"
    echo "Options:"
    echo "  1 - Runtime upgrade only"
    echo "  2 - Client upgrade only"
    echo "  3 - Runtime upgrade followed by client upgrade"
    echo "  4 - Client upgrade followed by runtime upgrade"
    echo "  5 - Start network given local parameters with no upgrade"
    exit 1
}
# Check for arguments
if [ "$#" -ne 1 ]; then
    echo "Error: You must provide exactly one argument."
    usage
fi
# Process the input option
case $1 in
    1)
        echo "Performing runtime upgrade only..."
        export_and_fork_state
        start_local_network
        echo "Waiting to allow the network to run and build up some state before proceeding to the next step..."
        sleep 30
        runtime_upgrade
        ;;
    2)
        echo "Performing client upgrade only..."
        export_and_fork_state
        start_local_network
        echo "Waiting to allow the network to run and build up some state before proceeding to the next step..."
        sleep 30
        client_upgrade
        ;;
    3)
        echo "Performing runtime upgrade followed by client upgrade..."
        export_and_fork_state
        start_local_network
        echo "Waiting to allow the network to run and build up some state before proceeding to the next step..."
        sleep 30
        runtime_upgrade
        client_upgrade
        ;;
    4)
        echo "Performing client upgrade followed by runtime upgrade..."
        export_and_fork_state
        start_local_network
        echo "Waiting to allow the network to run and build up some state before proceeding to the next step..."
        sleep 30
        client_upgrade
        runtime_upgrade
        ;;
    5)
        # export_and_fork_state
        start_local_network
        ;;
    *)
        echo "Invalid option: $1"
        usage
        ;;
esac
