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

# docs: docs_tests reads <root>/{res/cfg/default.toml (anchor), runtime/src/lib.rs,
# node/Cargo.toml, metadata/Cargo.toml, README.md, util/toolkit/src/fetcher/runtimes.rs,
# and lists docs/*.md to check they're linked in README.md}.
rm -rf docs/test-fixtures
mkdir -p docs/test-fixtures/res/cfg docs/test-fixtures/runtime/src docs/test-fixtures/node \
         docs/test-fixtures/metadata docs/test-fixtures/util/toolkit/src/fetcher docs/test-fixtures/docs
cp res/cfg/default.toml docs/test-fixtures/res/cfg/
cp runtime/src/lib.rs docs/test-fixtures/runtime/src/
cp node/Cargo.toml docs/test-fixtures/node/
cp metadata/Cargo.toml docs/test-fixtures/metadata/
cp README.md docs/test-fixtures/
cp util/toolkit/src/fetcher/runtimes.rs docs/test-fixtures/util/toolkit/src/fetcher/
cp docs/*.md docs/test-fixtures/docs/

# toolkit: unit tests walk to res/cfg/default.toml, then res("...") reads
# res/{cfg/*.toml, dev/*.json, test-tx-deserialize/serialized_tx.mn}. (test-data/**
# is util/toolkit's own package — globbed directly as same-package resources.)
rm -rf util/toolkit/test-fixtures
mkdir -p util/toolkit/test-fixtures/res util/toolkit/test-fixtures/util/toolkit/tests/cmd
# Shared res/ subset the unit tests (res()/include_str!) and the cli_tests corpus
# (../../res/...) read: cfg presets, dev configs, test-contract, serialized tx.
for d in cfg dev test-contract test-tx-deserialize; do
  cp -R "res/$d" util/toolkit/test-fixtures/res/
done
# genesis: only the *_undeployed.mn blobs are read (skip the ~3.8M of per-network genesis).
mkdir -p util/toolkit/test-fixtures/res/genesis
cp res/genesis/*_undeployed.mn util/toolkit/test-fixtures/res/genesis/
# cli_tests (trycmd) chdir's to test-fixtures/util/toolkit (mirrors the crate's 2-levels-
# deep path) and its tomls read ../../res/... -> test-fixtures/res/... and tests/cmd + README.
cp util/toolkit/tests/cmd/*.toml util/toolkit/test-fixtures/util/toolkit/tests/cmd/
cp util/toolkit/README.md util/toolkit/test-fixtures/util/toolkit/

echo "synced test fixtures:"
find node/test-fixtures docs/test-fixtures util/toolkit/test-fixtures -type f | sort
