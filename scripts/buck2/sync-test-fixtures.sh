#!/usr/bin/env bash
# Copy cross-package test fixtures into each consuming package's test-fixtures/ dir.
#
# Why: buck2 stages only SAME-PACKAGE `attrs.source` test `resources` onto RE
# workers; a cross-package target-ref resource (filegroup or even a mode=reference
# export_file whose output IS the source file) is never staged into the RE test
# action input. So a test that reads another package's files at runtime needs a
# committed local copy it can glob as same-package source. The consuming target
# sets MN_WORKSPACE_ROOT=<pkg>/test-fixtures so its workspace-root reads land here.
#
# Re-run this after changing any source-of-truth file below. CI runs it and
# `git diff --exit-code`s the test-fixtures dirs to catch drift.
set -euo pipefail
cd "$(dirname "$0")/../.."

# node: cfg::tests read <root>/res/cfg/*.toml ; openrpc::tests read <root>/docs/openrpc.json
rm -rf node/test-fixtures
mkdir -p node/test-fixtures/res/cfg node/test-fixtures/docs
cp res/cfg/*.toml node/test-fixtures/res/cfg/
cp docs/openrpc.json node/test-fixtures/docs/

echo "synced test fixtures:"
find node/test-fixtures -type f | sort
