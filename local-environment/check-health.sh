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

# This script returns a zero exit code if it finds a running midnight network at the given address. It works by checking the current block number.

timeout=360  # Default 6 minutes
address="http://localhost:9933"  # Default address
target_block="1"  # Default to block 1

while [[ $# -gt 0 ]]; do
    case $1 in
        -t|--timeout)
            if [[ ! "$2" =~ ^[0-9]+$ ]] || [[ "$2" -le 0 ]]; then
                echo "Error: timeout must be a positive integer"
                exit 1
            fi
            timeout="$2"
            shift 2
            ;;
        -u|--url)
            if [[ -z "$2" ]]; then
                echo "Error: URL cannot be empty"
                exit 1
            fi
            address="$2"
            shift 2
            ;;
        -b|--block)
            if [[ ! "$2" =~ ^(0x[0-9a-fA-F]+|[0-9]+)$ ]]; then
                echo "Error: block must be a number (decimal or hex)"
                exit 1
            fi
            target_block="$2"
            shift 2
            ;;
        *)
            echo "Usage: $0 [-t|--timeout SECONDS] [-u|--url URL] [-b|--block NUMBER]"
            exit 1
            ;;
    esac
done

start_time=$(date +%s)

while true; do
    result=$(curl -s -X POST "$address" \
        -H "Content-Type: application/json" \
        -d '{"jsonrpc":"2.0","id":1,"method":"chain_getHeader","params":[]}' \
        2>/dev/null | jq -r '.result.number' 2>/dev/null)

    if [[ -n "$result" && "$result" != "null" ]]; then
        # Convert hex to decimal for comparison
        result_dec=$((result))
        target_dec=$((target_block))
        [[ $result_dec -ge $target_dec ]] && break
    fi

    # Check timeout
    if (( $(date +%s) - start_time > timeout )); then
        echo "Timeout after ${timeout}s"
        exit 1
    fi

    sleep 1
done

echo "Block: $result"
