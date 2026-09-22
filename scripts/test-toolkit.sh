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

# Run Midnight Node Toolkit package tests
# Note: We use cargo nextest directly instead of cargo llvm-cov because
# llvm-cov applies -C instrument-coverage to WASM builds, which fails
# since WASM doesn't support profiler_builtins
#
# RUN_COMPACT_CONTRACT_TESTS (boolean, default false): "true" enables the slow
# contract E2E tests via the compact-contract-tests feature (set by the workflow of the same name).
FEATURES_ARG=""
if [ "${RUN_COMPACT_CONTRACT_TESTS:-false}" = "true" ]; then
    FEATURES_ARG="--features compact-contract-tests"
fi

MIDNIGHT_LEDGER_EXPERIMENTAL=1 cargo nextest run \
    --profile ci --release --locked \
    ${FEATURES_ARG} \
    -E 'package(midnight-node-toolkit)'
