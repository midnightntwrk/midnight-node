#!/usr/bin/env bash

set -eo pipefail

# Required by the native_e2e tests: the passwords at compile time (env!), the secret at
# runtime by the spawned indexer-api. See README "Testing" for details; values are arbitrary
# except APP__INFRA__SECRET which must be hex-encoded 32 bytes.
for var in APP__INFRA__STORAGE__PASSWORD APP__INFRA__PUB_SUB__PASSWORD APP__INFRA__SECRET; do
    if [ -z "${!var}" ]; then
        echo "Error: $var is not set." >&2
        echo "Fix: export it, e.g. in ~/.midnight-indexer.envrc or .envrc.local (see README \"Testing\")." >&2
        exit 1
    fi
done

# Trait declarations and thin binary entry points which cannot meaningfully be covered.
ignore_regex='(indexer-api/src/domain/storage|indexer-common/src/domain/pub_sub|indexer-api/src/bin/indexer-api-cli)'

# Install tooling and clean workspace.
rustup component add llvm-tools-preview
cargo llvm-cov clean --workspace

# First build tests without instrumentation.
cloud_tests=$(cargo test -p indexer-tests --test native_e2e --features cloud --no-run --message-format=json | jq -r 'select(.profile.test == true and .target.name == "native_e2e") | .executable')
standalone_tests=$(cargo test -p indexer-tests --test native_e2e --features standalone --no-run --message-format=json | jq -r 'select(.profile.test == true and .target.name == "native_e2e") | .executable')

# Then setup for coverage instrumentation and build the executables which are spawned in the tests.
source <(cargo llvm-cov show-env --export-prefix)
cargo build -p chain-indexer      --features cloud
cargo build -p wallet-indexer     --features cloud
cargo build -p indexer-api        --features cloud
cargo build -p indexer-standalone --features standalone

# Finally execute tests and create coverage report.
echo "Running tests for cloud feature"
"$cloud_tests" --no-capture
echo "Running tests for standalone feature"
"$standalone_tests" --no-capture
cargo llvm-cov report --html --ignore-filename-regex "$ignore_regex"
