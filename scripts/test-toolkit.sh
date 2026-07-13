#!/bin/sh
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
