#!/usr/bin/env bash
# Assemble node's cross-package test fixtures under node/test-fixtures/ at BUILD
# time (not as committed copies).
#
# Why a build-time copy and not a genrule or a cross-package resource: buck2
# stages only SAME-PACKAGE `attrs.source` test `resources` onto RE workers. A
# genrule-output resource and a cross-package target-ref resource are both NOT
# staged into the RE test action input (both tried, both red: the node unit test
# failed reading node/test-fixtures/res/cfg/default.toml on the worker). So the
# fixtures must exist as same-package source files that node/BUCK globs
# (`glob(["test-fixtures/**"])`) - but they need not be committed. This script
# materializes them from the //res and //docs source-of-truth; node/test-fixtures/
# is gitignored, so the tree carries no dupe.
#
# Run in Earthfile +buck2-prep before any `buck2 build/test`, and locally before
# `buck2 test //node/...`. It is NOT part of regen.sh/sync-test-fixtures.sh: those
# own the *committed*, drift-checked docs/ and toolkit/ fixtures, and node's
# gitignored output would otherwise read as regen drift (`diff -r` has no
# path-scoped exclude).
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."

# cfg::tests read <root>/res/cfg/*.toml ; openrpc::tests read <root>/docs/openrpc.json ;
# main_chain_follower tests read <root>/res/mock-bridge-data/ (mock authority registrations).
rm -rf node/test-fixtures
mkdir -p node/test-fixtures/res/cfg node/test-fixtures/docs node/test-fixtures/res/mock-bridge-data
cp res/cfg/*.toml node/test-fixtures/res/cfg/
cp docs/openrpc.json node/test-fixtures/docs/
cp res/mock-bridge-data/* node/test-fixtures/res/mock-bridge-data/

echo "staged node test fixtures:"
find node/test-fixtures -type f | sort
