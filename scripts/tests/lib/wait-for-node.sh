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

# wait_for_block <rpc_url> <target_block> [timeout_secs]
wait_for_block() {
    local url="$1"
    local target="$2"
    local timeout="${3:-90}"
    local elapsed=0
    echo "⏳ Waiting up to ${timeout}s for block ≥ ${target} at ${url}..."
    while [ "$elapsed" -lt "$timeout" ]; do
        local hex
        hex=$(curl -sf --max-time 2 -H 'Content-Type: application/json' \
            -d '{"jsonrpc":"2.0","method":"chain_getHeader","params":[],"id":1}' \
            "$url" 2>/dev/null \
            | grep -oE '"number":"0x[0-9a-fA-F]+"' \
            | grep -oE '0x[0-9a-fA-F]+')
        if [ -n "$hex" ] && [ "$((hex))" -ge "$target" ]; then
            echo "✅ Block $((hex)) ≥ ${target} after ${elapsed}s"
            return 0
        fi
        sleep 1
        elapsed=$((elapsed + 1))
    done
    echo "❌ Block ${target} not reached within ${timeout}s"
    return 1
}
