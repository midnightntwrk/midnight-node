#!/usr/bin/env bash

# This file is part of midnight-node.
# Copyright (C) Midnight Foundation
# SPDX-License-Identifier: Apache-2.0
# Licensed under the Apache License, Version 2.0 (the "License");
# You may not use this file except in compliance with the License.
# You may obtain a copy of the License at
# http://www.apache.org/licenses/LICENSE-2.0
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

# Builds the five `npm pack` tarballs the `compact-0.31.108` toolkit-js variant consumes via
# `file:` references (see util/toolkit-js/compact-0.31.108/vendor/README.md). They are built from
# the pinned `midnight-sdk` submodule — compact-js 2.5.3 is unpublished, and the matching
# `compact-runtime` is built from the SDK's nested `compact-submodule/runtime` via nix (it wraps the
# Rust/WASM onchain-runtime + zkir). `ledger-v9` is fetched from GitHub Packages (private).
#
#   compact-js.tgz          ── midnight-sdk/packages/compact-js          (compact-js 2.5.3)
#   compact-js-command.tgz  ── midnight-sdk/packages/compact-js-command  (compact-js 2.5.3)
#   compact-js-node.tgz     ── midnight-sdk/packages/compact-js-node     (compact-js 2.5.3)
#   compact-runtime.tgz     ── midnight-sdk/compact-submodule/runtime    (built via nix)
#   ledger-v9.tgz           ── @midnight-ntwrk/ledger-v9@0.1.0-alpha.1   (GitHub Packages)
#
# Output filenames are stable (independent of the internal package versions) so the variant's
# `file:vendor/<name>.tgz` references and the lockfile stay deterministic across re-pins.
#
# This is the build side only; it requires a GitHub Packages read token (for ledger-v9 and the SDK's
# install). The consume path (`npm ci` in util/toolkit-js) needs neither nix nor a token — the blobs
# are self-contained and everything else resolves from public npm.
#
# Designed to run inside the `+compact-js-bundle` Earthly target (FROM nixos/nix, IOG cache enabled),
# mirroring scripts/build-compactc.sh. Env knobs:
#   MIDNIGHTCI_PACKAGES_READ  GitHub Packages read token (required for ledger-v9 + SDK install)
#   SDK_DIR                   path to the midnight-sdk checkout      (default: <repo>/midnight-sdk)
#   VENDOR_DIR                where to write the five tarballs        (default: variant vendor/ dir)
#   LEDGER_V9_VERSION         ledger-v9 version to pack               (default: 0.1.0-alpha.1)

set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

SDK_DIR="${SDK_DIR:-${repo_root}/midnight-sdk}"
VENDOR_DIR="${VENDOR_DIR:-${repo_root}/util/toolkit-js/compact-0.31.108/vendor}"
LEDGER_V9_VERSION="${LEDGER_V9_VERSION:-0.1.0-alpha.1}"

# Relative paths inside the SDK. The compact-js packages reference the runtime as
# `file:../../compact-submodule/runtime`, i.e. they live two levels below the SDK root.
RUNTIME_DIR="${SDK_DIR}/compact-submodule/runtime"
COMPACT_JS_PKGS=(
  "packages/compact-js:compact-js"
  "packages/compact-js-command:compact-js-command"
  "packages/compact-js-node:compact-js-node"
)

if [ ! -d "$SDK_DIR" ]; then
  echo "error: midnight-sdk submodule not found at ${SDK_DIR}." >&2
  echo "       Run: git submodule update --init --recursive midnight-sdk" >&2
  exit 1
fi
if [ ! -d "$RUNTIME_DIR" ]; then
  echo "error: compact runtime not found at ${RUNTIME_DIR}." >&2
  echo "       The midnight-sdk nested 'compact-submodule' must be checked out (recursive)." >&2
  exit 1
fi
for tool in nix yarn npm node; do
  command -v "$tool" >/dev/null 2>&1 || { echo "error: ${tool} is required." >&2; exit 1; }
done

mkdir -p "$VENDOR_DIR"

# GitHub Packages auth for the @midnight-ntwrk scope (ledger-v9 + any private SDK deps). Written to a
# user-level .npmrc so both npm and yarn pick it up; never bundled into the produced tarballs.
if [ -n "${MIDNIGHTCI_PACKAGES_READ:-}" ]; then
  npmrc="${HOME}/.npmrc"
  {
    echo "@midnight-ntwrk:registry=https://npm.pkg.github.com"
    echo "//npm.pkg.github.com/:_authToken=${MIDNIGHTCI_PACKAGES_READ}"
  } >> "$npmrc"
  echo "Configured GitHub Packages auth for @midnight-ntwrk in ${npmrc}"
else
  echo "warning: MIDNIGHTCI_PACKAGES_READ is unset; ledger-v9 fetch and SDK install will likely fail." >&2
fi

# pack_to <package-dir> <stable-name>: `npm pack` the package and move the produced tarball to
# <VENDOR_DIR>/<stable-name>.tgz (npm prints the produced filename on stdout).
pack_to() {
  local pkg_dir="$1" stable_name="$2"
  local produced
  produced="$(cd "$pkg_dir" && npm pack --silent --pack-destination "$VENDOR_DIR")"
  mv -f "${VENDOR_DIR}/${produced}" "${VENDOR_DIR}/${stable_name}.tgz"
  echo "packed ${pkg_dir} -> ${VENDOR_DIR}/${stable_name}.tgz"
}

echo "==> Building compact-runtime from ${RUNTIME_DIR} via nix (first build can be slow)"
# The runtime build wraps the Rust/WASM onchain-runtime + zkir; nix provides that toolchain and the
# IOG cache provides zkir prebuilt. `path:` ref because the COPY'd submodule has no .git in CI.
(
  cd "$SDK_DIR"
  nix develop "path:${SDK_DIR}#compact-runtime" --command bash -c "cd compact-submodule/runtime && yarn install --frozen-lockfile && yarn build"
)

# Concrete runtime version to pin the compact-js packages against (read from the built package.json).
RUNTIME_VER="$(node -p "require('${RUNTIME_DIR}/package.json').version")"
echo "compact-runtime version: ${RUNTIME_VER}"
pack_to "$RUNTIME_DIR" "compact-runtime"

echo "==> Installing & building the compact-js packages in ${SDK_DIR}"
(
  cd "$SDK_DIR"
  yarn install --frozen-lockfile
  yarn turbo run build --filter='@midnight-ntwrk/compact-js' \
                       --filter='@midnight-ntwrk/compact-js-command' \
                       --filter='@midnight-ntwrk/compact-js-node'
)

# Before packing, rewrite each package's `@midnight-ntwrk/compact-runtime` dependency from the
# in-repo `file:../../compact-submodule/runtime` link to the concrete built version, so the published
# tarball declares a real version range that our vendored compact-runtime.tgz satisfies.
rewrite_runtime_dep() {
  local pkg_json="$1" ver="$2"
  node -e '
    const fs = require("fs");
    const [file, ver] = [process.argv[1], process.argv[2]];
    const pkg = JSON.parse(fs.readFileSync(file, "utf8"));
    for (const field of ["dependencies", "peerDependencies", "optionalDependencies"]) {
      if (pkg[field] && pkg[field]["@midnight-ntwrk/compact-runtime"]) {
        pkg[field]["@midnight-ntwrk/compact-runtime"] = ver;
      }
    }
    fs.writeFileSync(file, JSON.stringify(pkg, null, 2) + "\n");
  ' "$pkg_json" "$ver"
}

for entry in "${COMPACT_JS_PKGS[@]}"; do
  rel="${entry%%:*}"
  stable="${entry##*:}"
  pkg_dir="${SDK_DIR}/${rel}"
  rewrite_runtime_dep "${pkg_dir}/package.json" "$RUNTIME_VER"
  pack_to "$pkg_dir" "$stable"
done

echo "==> Packing ledger-v9@${LEDGER_V9_VERSION} from GitHub Packages"
produced="$(npm pack "@midnight-ntwrk/ledger-v9@${LEDGER_V9_VERSION}" --silent --pack-destination "$VENDOR_DIR")"
mv -f "${VENDOR_DIR}/${produced}" "${VENDOR_DIR}/ledger-v9.tgz"
echo "packed @midnight-ntwrk/ledger-v9@${LEDGER_V9_VERSION} -> ${VENDOR_DIR}/ledger-v9.tgz"

echo ""
echo "Done. Vendored tarballs in ${VENDOR_DIR}:"
ls -la "${VENDOR_DIR}"/*.tgz
