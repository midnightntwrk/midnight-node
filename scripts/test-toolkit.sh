#!/bin/sh
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

# toolkit_e2e's tests share one node and a pool of genesis-funded seeds, so they must run in
# one process: libtest, not nextest's process per test.
mkdir -p "$EXTRACT_DIR"
cargo nextest list --archive-file "$ARCHIVE" --workspace-remap / --extract-to "$EXTRACT_DIR" > /dev/null
E2E_BIN=$(find "$EXTRACT_DIR/target/release/deps" -maxdepth 1 -type f -executable -name 'toolkit_e2e-*')

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
