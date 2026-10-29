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
ART="$ROOT/artifacts/snowbridge"
[[ -z "${SKIP_WASM_BUILD:-}" ]] || { echo "Unset SKIP_WASM_BUILD: this validation requires real Wasm" >&2; exit 1; }
[[ -f "$ART/compact-ledger/escrow-state.bin" ]] || { echo "Run tests/snowbridge/test.sh first" >&2; exit 1; }
export CARGO_TARGET_DIR="$ROOT/target"
cargo test --locked -p pallet-midnight-system --lib 2>&1 | tee "$ART/system-regression.log"
cargo test --locked -p pallet-ethereum-bridge --lib 2>&1 | tee "$ART/pallet-tests.log"
cargo test --locked --manifest-path tests/snowbridge/rust/Cargo.toml write_fresh_local_node_genesis 2>&1 | tee "$ART/genesis-test.log"
python3 tests/snowbridge/prepare-node.py
(
    cd "$ART/node"
    # The staged manifest deliberately replaces two git dependencies with patched paths.
    cargo test -p midnight-node-ledger --test ethereum_escrow --features ethereum-bridge-poc -- --ignored --nocapture 2>&1 | tee "$ART/host-tests.log"
    cargo build --locked --release -p midnight-node --features ethereum-bridge-poc 2>&1 | tee "$ART/node-build.log"
    # The release build above already verifies real Wasm. This regression runs natively.
    SKIP_WASM_BUILD=1 cargo test --locked -p midnight-node-runtime --features ethereum-bridge-poc bridge 2>&1 | tee "$ART/runtime-filter-tests.log"
)
python3 tests/snowbridge/node-smoke.py 2>&1 | tee "$ART/node-smoke.log"
python3 tests/snowbridge/node-report.py
