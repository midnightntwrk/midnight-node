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

# Regenerate every @generated buck2 file from the cargo workspace: the
# third-party and wasm-deps universes and each member's BUCK. CI runs this and
# fails on any diff (Earthfile +buck2-check-regen), so a Cargo.lock bump that
# skips it goes red instead of building stale crates. Refresh with
# `earth +buck2-regen` (the pinned reindeer), or run this directly with REINDEER
# pointing at a build of that rev.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."
REINDEER=${REINDEER:-reindeer}

# buckify asks crates.io for every tarball's size, and one 503 in ~1600
# requests fails the run: retry.
buckify() {
    local attempt
    for attempt in 1 2 3; do
        "$REINDEER" --third-party-dir "$1" buckify && return
        echo "regen: buckify $1 failed (attempt $attempt of 3)" >&2
        sleep $((attempt * 10))
    done
    return 1
}

# What clippy lints on top of the default features (root//platforms:lint).
LINT_FEATURES=runtime-benchmarks,try-runtime

# Features CI turns on to run integration tests that name them in required-features.
TEST_FEATURES=pallet-midnight/test-utils

meta=$(mktemp)
lint_meta=$(mktemp)
test_meta=$(mktemp)
trap 'rm -f "$meta" "$lint_meta" "$test_meta"' EXIT
cargo metadata --format-version 1 --locked > "$meta"
cargo metadata --format-version 1 --locked --features "$LINT_FEATURES" > "$lint_meta"
cargo metadata --format-version 1 --locked --features "$TEST_FEATURES" > "$test_meta"

# Both universes' locks derive from the workspace lock. Drop this one before
# gen-third-party.py so it reseeds (it only seeds a missing lock); gen-wasm-deps.sh
# reseeds its own unconditionally. Otherwise a workspace bump never reaches the graph.
rm -f third-party/Cargo.lock
scripts/buck2/gen-third-party.py "$meta" third-party/Cargo.toml "$lint_meta"
buckify third-party
scripts/buck2/normalize-wasm-stub-outdir.py third-party/BUCK

scripts/buck2/gen-wasm-deps.sh
buckify wasm-deps

scripts/buck2/gen-first-party.py "$meta" "$lint_meta" "$test_meta"
scripts/buck2/check-fixups.py
