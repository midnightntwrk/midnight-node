#!/usr/bin/env bash
#
# Run the toolkit-js test suite against every supported compactc version.
#
# For each version we recompile the test contract(s) with that compactc (so the generated `managed/`
# output expects that version's `compact-runtime`), then run vitest with COMPACTC_VERSION set. The test
# setup (`test/setup-compactc-resolver.ts`) installs the same module-resolution hook the CLI uses, so
# every `compact-js*` / `compact-runtime` import resolves to the matching variant workspace.
#
# Usage:
#   ./scripts/test-all-compactc.sh                 # all supported versions
#   ./scripts/test-all-compactc.sh 0.30.0 0.31.0   # a subset

set -euo pipefail

cd "$(dirname "$0")/.."

# In a dev shell `.envrc` exports COMPACT_HOME to the single submodule-built compiler, and
# `run-compactc`/`fetch-compactc` honour COMPACT_HOME over COMPACTC_VERSION — so leaving it set would
# pin every iteration below to that one compiler and silently defeat the whole compatibility suite.
# Unset it so each iteration fetches and compiles with the COMPACTC_VERSION it actually requested.
unset COMPACT_HOME

# Concrete patch versions to fetch for each supported <major>.<minor> line (see SUPPORTED_COMPACTC_VERSIONS
# in src/compactc-resolver.ts). Override by passing versions as arguments.
DEFAULT_VERSIONS=("0.29.0" "0.30.0" "0.31.0")
if [ "$#" -gt 0 ]; then
  VERSIONS=("$@")
else
  VERSIONS=("${DEFAULT_VERSIONS[@]}")
fi

# Build the variant workspaces once; the resolver imports their built dist/.
npm run build:variants

failures=()
for version in "${VERSIONS[@]}"; do
  echo ""
  echo "==================================================================="
  echo "  Testing against compactc ${version}"
  echo "==================================================================="
  export COMPACTC_VERSION="${version}"

  # Recompile the test contract(s) with this compactc version so the managed output matches the runtime
  # the variant provides. Removing the existing output forces `build-compact` to recompile.
  rm -rf ./test/contract/managed ./test/minter_contract/out ./mint/out
  npm run build-compact

  if npx vitest run; then
    echo "PASS: compactc ${version}"
  else
    echo "FAIL: compactc ${version}"
    failures+=("${version}")
  fi
done

echo ""
echo "==================================================================="
if [ "${#failures[@]}" -ne 0 ]; then
  echo "FAILED versions: ${failures[*]}"
  exit 1
fi
echo "All ${#VERSIONS[@]} compactc version(s) passed: ${VERSIONS[*]}"
