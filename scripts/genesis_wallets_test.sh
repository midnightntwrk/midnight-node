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
    NODE_CONTAINER="midnight-node-genesis"
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
SEEDS_FILE="${SEEDS_FILE:-$REPO_ROOT/res/dev/undeployed-genesis-seeds.json}"

# Prefer an explicit SEEDS override; otherwise load the funded undeployed set from
# res/dev/undeployed-genesis-seeds.json (twenty wallets). Callers for other networks
# can pass SEEDS / SEEDS_FILE as needed.
if [[ -n "${SEEDS:-}" ]]; then
    read -r -a seeds <<< "$SEEDS"
elif [[ -f "$SEEDS_FILE" ]]; then
    mapfile -t seeds < <(python3 -c '
import json, re, sys
obj = json.load(open(sys.argv[1]))
def key(k):
    m = re.fullmatch(r"wallet-seed-(\d+)", k)
    return (0, int(m.group(1))) if m else (1, k)
for k in sorted(obj, key=key):
    if k.startswith("wallet-seed-"):
        print(obj[k])
' "$SEEDS_FILE")
else
    echo "No SEEDS override and seeds file not found: $SEEDS_FILE" >&2
    exit 1
fi

check_seeds() {
    local command=$1
    local success=true

    echo "Checking seeds using command: $command"
    for seed in "${seeds[@]}"; do
        if ! output=$(docker run --network "$NETWORK" "$TOOLKIT_IMAGE" $command --seed "$seed" --src-url "ws://${NODE_CONTAINER}:9944"); then
            echo "Toolkit '$command' failed for seed $seed"
            success=false
            continue
        fi

        # A successful run must still contain the JSON utxos report — anything
        # else (e.g. an output-format change) is a failure, not a funded wallet.
        if ! echo "$output" | grep -q '"utxos"'; then
            echo "No utxos report in output for seed $seed"
            success=false
            continue
        fi

        # An empty unshielded UTXO set means the wallet is unfunded
        if echo "$output" | grep -q '"utxos": \[\]'; then
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
