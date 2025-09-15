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

if [[ -z $TOOLKIT_IMAGE ]]; then
    echo "Building container..."
    earthly +toolkit-image
    TOOLKIT_IMAGE="ghcr.io/midnight-ntwrk/midnight-node-toolkit:latest"
fi

if [[ -z $NETWORK ]]; then
    echo "Missing NETWORK variable, defaulting to 'midnight-net-genesis'"
    NETWORK="midnight-net-genesis"
fi

if [[ -z $NODE_CONTAINER ]]; then
    echo "Missing NODE_CONTAINER variable, defaulting to 'midnight-node-genesis'"
    NETWORK="midnight-node-genesis"
fi

seeds=("0000000000000000000000000000000000000000000000000000000000000001" "0000000000000000000000000000000000000000000000000000000000000002" "0000000000000000000000000000000000000000000000000000000000000003" "0000000000000000000000000000000000000000000000000000000000000004")
check_seeds() {
    local command=$1
    local success=true
    
    echo "Checking seeds using command: $command"
    for seed in ${seeds[@]}; do
        output=$(docker run --network $NETWORK $TOOLKIT_IMAGE $command --seed $seed --src-url ws://${NODE_CONTAINER}:9944)
        
        # Check if coins field is empty using grep
        if echo "$output" | grep -q "Unshielded UTXOs: \[[[:space:]]*\]"; then
            echo "Wallet for seed $seed has an empty UTXOs list"
            success=false
            continue
        fi
    done
    echo "Finished checking with $command"
    return $([ "$success" = "true" ])
}

# Check both wallet derivations
check_seeds "show-wallet"
wallet_result=$?

# Exit with error if either check failed
if [ $wallet_result -eq 0 ]; then
    echo "All seeds have proper funding in both wallet derivations"
    exit 0
else
    echo "Some seeds are missing proper funding"
    exit 1
fi
