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

set -euxo pipefail

# The archive from +toolkit-test-archive fixes the feature set (RUN_COMPACT_CONTRACT_TESTS is a build ARG).
ARCHIVE=/toolkit-tests.tar.zst
EXTRACT_DIR=/tmp/toolkit-tests

# Missing runtime assets must fail CI rather than silently skip contract tests.
test -s /Cargo.toml
test -s /util/toolkit/README.md
test -s /util/toolkit/examples/single-tx.md
test -s /util/toolkit-js/dist/bin.js
test -s /util/toolkit/test-images.docker-compose.yml
test -s /util/toolkit/test-data/genesis/genesis_block_undeployed.mn
test -s /util/toolkit/tests/contracts/counter/counter.compact
test -s /.config/nextest.toml
test -s /res/dev/undeployed-genesis-seeds.json
compact_version="${COMPACTC_VERSION%%[-+]*}"
test -d "/util/toolkit-js/node_modules/@midnight-ntwrk/node-toolkit-compact-$compact_version"
test "$("$COMPACT_HOME/compactc" --version | cut -d ' ' -f 1)" = "$compact_version"

# toolkit_e2e's tests share one node and a pool of genesis-funded seeds, so they must run in
# one process: libtest, not nextest's process per test.
mkdir -p "$EXTRACT_DIR"
cargo nextest list --archive-file "$ARCHIVE" --workspace-remap / --extract-to "$EXTRACT_DIR" > /dev/null
# No fixed subdir: cargo <1.100 puts test binaries in release/deps, 1.100+ in
# release/build/<pkg>/<hash>/out.
E2E_BIN=$(find "$EXTRACT_DIR/target/release" -type f -executable -name 'toolkit_e2e-*')
if [ -z "$E2E_BIN" ] || [ "$(echo "$E2E_BIN" | wc -l)" -ne 1 ]; then
    echo "error: expected one toolkit_e2e-* test binary under $EXTRACT_DIR/target/release, found: '${E2E_BIN}'" >&2
    exit 1
fi

cargo nextest run --profile ci --workspace-remap / \
    --binaries-metadata "$EXTRACT_DIR/target/nextest/binaries-metadata.json" \
    --cargo-metadata "$EXTRACT_DIR/target/nextest/cargo-metadata.json" \
    --target-dir-remap "$EXTRACT_DIR/target" \
    -E 'not binary(toolkit_e2e)' &
NEXTEST_PID=$!

status=0
(cd /util/toolkit && "$E2E_BIN" --test-threads=16 2>&1 | sed -u 's/^/[toolkit_e2e] /') || status=1
wait "$NEXTEST_PID" || status=1
exit "$status"
