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
COMPACTC=${COMPACTC:-$ROOT/artifacts/snowbridge/tools/compact/compactc}
FORGE=${FORGE:-$ROOT/artifacts/snowbridge/tools/foundry/forge}
[[ "$($COMPACTC --version)" == 0.34.0 ]] || { echo "Compact 0.34.0 required" >&2; exit 1; }
node -e 'if (Number(process.versions.node.split(".")[0]) < 22) throw new Error("Use Node 22+ (validated with 24.14.0)")'
for contract in escrow consumer; do
    "$COMPACTC" "tests/snowbridge/compact/$contract.compact" "artifacts/snowbridge/compact/$contract"
done
npm test --prefix tests/snowbridge
cargo test --locked --manifest-path tests/snowbridge/sp1/Cargo.toml -p midnight-beefy
node tests/snowbridge/compact-ledger.mjs
export SNOWBRIDGE_SP1_PROOF=${SNOWBRIDGE_SP1_PROOF:-$ROOT/tests/snowbridge/fixtures/sp1-committee-boundary-proof.json}
export SNOWBRIDGE_SP1_INITIAL=${SNOWBRIDGE_SP1_INITIAL:-$ROOT/tests/snowbridge/fixtures/sp1-committee-boundary-initial.scale}
"$FORGE" test --root tests/snowbridge/contracts -vv
MIDNIGHT_LEDGER_TEST_STATIC_DIR="$ROOT/artifacts/snowbridge/compact" CARGO_TARGET_DIR="$ROOT/target" cargo test --locked --manifest-path tests/snowbridge/rust/Cargo.toml --features compact-ledger
node tests/snowbridge/verify-escrow.mjs
