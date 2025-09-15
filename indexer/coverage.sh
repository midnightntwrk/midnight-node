#!/usr/bin/env bash

set -eo pipefail

if [ -z "$1" ]; then
    echo "Error: Rust nightly version parameter is required" >&2
    echo "Usage: $0 <nightly>" >&2
    exit 1
fi
nightly="$1"

# Install tooling and clean workspace.
rustup component add llvm-tools-preview --toolchain $nightly
cargo llvm-cov clean --workspace

# First build tests without instrumentation.
cloud_tests=$(cargo test -p indexer-tests --test native_e2e --features cloud --no-run --message-format=json | jq -r 'select(.profile.test == true and .target.name == "native_e2e") | .executable')
standalone_tests=$(cargo test -p indexer-tests --test native_e2e --features standalone --no-run --message-format=json | jq -r 'select(.profile.test == true and .target.name == "native_e2e") | .executable')

# Then setup for coverage instrumentation and build the executables which are spawned in the tests.
source <(cargo +$nightly llvm-cov show-env --export-prefix)
cargo +$nightly build -p chain-indexer      --features cloud
cargo +$nightly build -p wallet-indexer     --features cloud
cargo +$nightly build -p indexer-api        --features cloud
cargo +$nightly build -p indexer-standalone --features standalone

# Finally execute tests and create coverage report.
echo "Running tests for cloud feature"
"$cloud_tests" --no-capture
echo "Running tests for standalone feature"
"$standalone_tests" --no-capture
cargo llvm-cov report --html
