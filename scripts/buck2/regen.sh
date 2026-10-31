#!/usr/bin/env bash
# Regenerate every @generated buck2 file from the cargo workspace: the
# third-party and wasm-deps universes, each member's BUCK and the copied test
# fixtures. CI runs this and fails on any diff (Earthfile +buck2-check-regen),
# so a Cargo.lock bump that skips it goes red instead of building stale
# crates. Refresh with `earth +buck2-regen` (the pinned reindeer), or run this
# directly with REINDEER pointing at a build of that rev.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."
REINDEER=${REINDEER:-reindeer}

meta=$(mktemp)
trap 'rm -f "$meta"' EXIT
cargo metadata --format-version 1 --locked > "$meta"

# Both universes' locks derive from the workspace lock. Drop this one before
# gen-third-party.py so it reseeds (it only seeds a missing lock); gen-wasm-deps.sh
# reseeds its own unconditionally. Otherwise a workspace bump never reaches the graph.
rm -f third-party/Cargo.lock
scripts/buck2/gen-third-party.py "$meta" third-party/Cargo.toml
"$REINDEER" --third-party-dir third-party buckify
scripts/buck2/normalize-git-sources.py third-party
scripts/buck2/normalize-wasm-stub-outdir.py third-party/BUCK

scripts/buck2/gen-wasm-deps.sh
"$REINDEER" --third-party-dir wasm-deps buckify
scripts/buck2/normalize-git-sources.py wasm-deps

scripts/buck2/gen-first-party.py "$meta"
scripts/buck2/sync-test-fixtures.sh
