#!/bin/sh
set -euxo pipefail

MIDNIGHT_LEDGER_EXPERIMENTAL=1 cargo llvm-cov nextest \
    --profile ci --release --locked \
    -E 'package(midnight-node-toolkit)'

cargo llvm-cov report --html --release \
    --output-dir "/test-artifacts-toolkit-${NATIVEARCH}/html"

cargo llvm-cov report --lcov --release \
    --output-path "/test-artifacts-toolkit-${NATIVEARCH}/tests.lcov"
