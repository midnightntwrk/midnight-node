#!/usr/bin/env bash
# This file is part of midnight-node.
# Copyright (C) Midnight Foundation
# SPDX-License-Identifier: Apache-2.0
# Licensed under the Apache License, Version 2.0 (the "License");
# You may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#	http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

set -euo pipefail
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
cd "$ROOT"
ANVIL=${ANVIL:-$ROOT/artifacts/snowbridge/tools/foundry/anvil}
FORGE=${FORGE:-$ROOT/artifacts/snowbridge/tools/foundry/forge}
PORT=${SNOWBRIDGE_PORT:-18545}
RPC="http://127.0.0.1:$PORT"
python3 - "$PORT" <<'PYTHON'
import socket,sys
with socket.socket() as probe:
    probe.bind(("127.0.0.1", int(sys.argv[1])))
PYTHON
"$ANVIL" --host 127.0.0.1 --port "$PORT" --chain-id 31337 --hardfork prague --gas-limit 30000000 --silent > artifacts/snowbridge/anvil.log 2>&1 &
ANVIL_PID=$!
trap 'kill "$ANVIL_PID" 2>/dev/null || true; wait "$ANVIL_PID" 2>/dev/null || true' EXIT
python3 - "$RPC" <<'PYTHON'
import json,sys,time,urllib.request
for attempt in range(50):
    try:
        request=urllib.request.Request(sys.argv[1],json.dumps({"jsonrpc":"2.0","id":1,"method":"eth_chainId","params":[]}).encode(),{"Content-Type":"application/json"})
        with urllib.request.urlopen(request,timeout=1) as response:
            assert int(json.load(response)["result"],16)==31337
        break
    except (OSError, AssertionError):
        time.sleep(0.1)
else:
    raise SystemExit("Anvil did not become ready")
PYTHON
cd "$ROOT/tests/snowbridge/contracts"
"$FORGE" script script/Local.s.sol:Local --no-dynamic-test-linking --rpc-url "$RPC" \
    --broadcast --unlocked --sender 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266 --slow
cd "$ROOT"
python3 tests/snowbridge/observe.py --rpc "$RPC"
SNOWBRIDGE_DEPOSIT_LOGS="$ROOT/artifacts/snowbridge/deposit-logs.json" CARGO_TARGET_DIR="$ROOT/target" \
    cargo test --locked --manifest-path tests/snowbridge/rust/Cargo.toml solidity_registration_and_deposit_decode_with_the_pinned_sdk
