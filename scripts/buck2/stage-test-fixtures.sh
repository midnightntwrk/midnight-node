#!/usr/bin/env bash
# Assemble every package's cross-package buck2 test fixtures under
# <pkg>/test-fixtures/ at BUILD time (not as committed copies).
#
# Why a build-time copy and not a committed tree or a genrule: buck2 stages only
# SAME-PACKAGE `attrs.source` test `resources` onto RE workers - a genrule output
# and a cross-package target-ref are both NOT staged into the RE test action input
# (both tried, both red). So a test that reads another package's files at runtime
# needs those files as same-package source the package's BUCK can glob
# (`glob(["test-fixtures/**"])`) - but they need not be committed. This script
# materializes them from the //res, //docs and workspace source-of-truth; each
# <pkg>/test-fixtures/ is gitignored, so the tree carries no dupe.
#
# cargo is unaffected: the tests resolve their fixture root from MN_WORKSPACE_ROOT
# / MN_CLI_FIXTURES_ROOT when buck2 sets it, and otherwise fall back to the real
# repo tree (util/toolkit/src/lib.rs test_paths, docs/tests/docs_tests.rs,
# util/toolkit/tests/cli_tests.rs). So only buck2 reads test-fixtures/.
#
# Run in Earthfile +buck2-prep before any `buck2 build/test`, and locally before
# `buck2 test //...`. It is deliberately NOT part of regen.sh: the regen check
# diffs /tmp/before against /work with `diff -r`, which has no path-scoped exclude,
# so a gitignored, build-time-only tree would read as regen drift.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."

# ── node ─────────────────────────────────────────────────────────────────────
# cfg::tests read <root>/res/cfg/*.toml ; openrpc::tests read <root>/docs/openrpc.json ;
# main_chain_follower tests read <root>/res/mock-bridge-data/ (mock authority registrations).
rm -rf node/test-fixtures
mkdir -p node/test-fixtures/res/cfg node/test-fixtures/docs node/test-fixtures/res/mock-bridge-data
cp res/cfg/*.toml node/test-fixtures/res/cfg/
cp docs/openrpc.json node/test-fixtures/docs/
cp res/mock-bridge-data/* node/test-fixtures/res/mock-bridge-data/

# ── docs ─────────────────────────────────────────────────────────────────────
# docs_tests reads <root>/{res/cfg/default.toml (anchor), runtime/src/lib.rs,
# node/Cargo.toml, metadata/Cargo.toml, README.md, util/toolkit/src/fetcher/runtimes.rs,
# and lists docs/*.md to check they're linked in README.md}.
rm -rf docs/test-fixtures
mkdir -p docs/test-fixtures/res/cfg docs/test-fixtures/runtime/src docs/test-fixtures/node \
         docs/test-fixtures/metadata docs/test-fixtures/util/toolkit/src/fetcher docs/test-fixtures/docs
cp res/cfg/default.toml docs/test-fixtures/res/cfg/
cp runtime/src/lib.rs docs/test-fixtures/runtime/src/
cp runtime/Cargo.toml docs/test-fixtures/runtime/
cp node/Cargo.toml docs/test-fixtures/node/
cp metadata/Cargo.toml docs/test-fixtures/metadata/
cp README.md docs/test-fixtures/
cp util/toolkit/src/fetcher/runtimes.rs docs/test-fixtures/util/toolkit/src/fetcher/
cp docs/*.md docs/test-fixtures/docs/

# ── util/toolkit ─────────────────────────────────────────────────────────────
# unit tests walk to res/cfg/default.toml, then res("...") reads
# res/{cfg/*.toml, dev/*.json, test-contract/*, test-tx-deserialize/serialized_tx.mn}.
# (test-data/** is util/toolkit's own package - globbed directly as same-package.)
rm -rf util/toolkit/test-fixtures
mkdir -p util/toolkit/test-fixtures/res util/toolkit/test-fixtures/util/toolkit/tests/cmd
for d in cfg dev test-contract test-tx-deserialize; do
  cp -R "res/$d" util/toolkit/test-fixtures/res/
done
# genesis: only the *_undeployed.mn blobs are read (skip the ~3.8M of per-network genesis).
mkdir -p util/toolkit/test-fixtures/res/genesis
cp res/genesis/*_undeployed.mn util/toolkit/test-fixtures/res/genesis/
# cli_tests (trycmd) chdir to MN_CLI_FIXTURES_ROOT (a mirror 2 levels deep) and its
# tomls read ../../res/... -> test-fixtures/res/..., plus tests/cmd + README.
cp util/toolkit/tests/cmd/*.toml util/toolkit/test-fixtures/util/toolkit/tests/cmd/
cp util/toolkit/README.md util/toolkit/test-fixtures/util/toolkit/

echo "staged buck2 test fixtures:"
find node/test-fixtures docs/test-fixtures util/toolkit/test-fixtures -type f | sort
