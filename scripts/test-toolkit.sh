#!/bin/sh
set -euxo pipefail

# Run Midnight Node Toolkit package tests
# Note: We use cargo nextest directly instead of cargo llvm-cov because
# llvm-cov applies -C instrument-coverage to WASM builds, which fails
# since WASM doesn't support profiler_builtins
MIDNIGHT_LEDGER_EXPERIMENTAL=1 cargo nextest run \
    --profile ci --release --locked \
    -E 'package(midnight-node-toolkit)'
