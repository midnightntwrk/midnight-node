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
# `compact-runtime` (0.16.x, well past the 0.16.0 public-npm ceiling) is built from the SDK's nested
# `compact-submodule/runtime` via nix (the build runs a Chez-Scheme step that wraps the onchain
# runtime + zkir). `ledger-v9` is fetched from GitHub Packages (private).
#
#   compact-js.tgz          ── midnight-sdk/compact-js/compact-js          (compact-js 2.5.3)
#   compact-js-command.tgz  ── midnight-sdk/compact-js/compact-js-command  (compact-js 2.5.3)
#   compact-js-node.tgz     ── midnight-sdk/compact-js/compact-js-node     (compact-js 2.5.3)
#   compact-runtime.tgz     ── midnight-sdk/compact-submodule/runtime      (built via nix)
#   ledger-v9.tgz           ── @midnight-ntwrk/ledger-v9@0.1.0-alpha.1     (GitHub Packages)
#
# Output filenames are stable (independent of the internal package versions) so the variant's
# `file:vendor/<name>.tgz` references and the lockfile stay deterministic across re-pins.
#
# This is the build side only; it requires a GitHub Packages read token (for ledger-v9 and the SDK's
# yarn install). The consume path (`npm ci` in util/toolkit-js) needs neither nix nor a token — the
# blobs are self-contained and everything else resolves from public npm.
#
# Designed to run inside the `+compact-js-bundle` Earthly target (FROM nixos/nix, IOG cache enabled),
# mirroring scripts/build-compactc.sh. The host needs only Earthly + Docker; nix/yarn live inside the
# build. Env knobs:
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

# The compact-js packages share a yarn workspace rooted at midnight-sdk/compact-js/. They reference
# the runtime as `file:../../compact-submodule/runtime` (i.e. compact-js/<pkg> → ../../ → SDK root).
COMPACT_JS_ROOT="${SDK_DIR}/compact-js"
RUNTIME_DIR="${SDK_DIR}/compact-submodule/runtime"
# <stable-name>:<dir under midnight-sdk/compact-js>
COMPACT_JS_PKGS=(
  "compact-js:compact-js"
  "compact-js-command:compact-js-command"
  "compact-js-node:compact-js-node"
)

if [ ! -d "$COMPACT_JS_ROOT" ]; then
  echo "error: midnight-sdk compact-js workspace not found at ${COMPACT_JS_ROOT}." >&2
  echo "       Run: git submodule update --init --recursive midnight-sdk" >&2
  exit 1
fi
if [ ! -d "$RUNTIME_DIR" ]; then
  echo "error: compact runtime not found at ${RUNTIME_DIR}." >&2
  echo "       The midnight-sdk nested 'compact-submodule' must be checked out (recursive)." >&2
  exit 1
fi
for tool in nix node npm; do
  command -v "$tool" >/dev/null 2>&1 || { echo "error: ${tool} is required." >&2; exit 1; }
done

mkdir -p "$VENDOR_DIR"

# --- GitHub Packages auth for the @midnight-ntwrk scope (ledger-v9 + the SDK's yarn install) --------
# Never bundled into the produced tarballs; used only during this build.
if [ -z "${MIDNIGHTCI_PACKAGES_READ:-}" ]; then
  echo "warning: MIDNIGHTCI_PACKAGES_READ is unset; ledger-v9 fetch and SDK install will likely fail." >&2
fi
# npm side (npm pack of ledger-v9):
{
  echo "@midnight-ntwrk:registry=https://npm.pkg.github.com"
  echo "//npm.pkg.github.com/:_authToken=${MIDNIGHTCI_PACKAGES_READ:-}"
} >> "${HOME}/.npmrc"
# yarn side: the project .yarnrc.yml scopes @midnight-ntwrk to npm.pkg.github.com with npmAlwaysAuth
# but carries no token. Append a top-level npmRegistries block (a new key — no conflict with the
# existing npmScopes map) so yarn authenticates against that registry.
if [ -n "${MIDNIGHTCI_PACKAGES_READ:-}" ]; then
  cat >> "${COMPACT_JS_ROOT}/.yarnrc.yml" <<EOF

npmRegistries:
  "https://npm.pkg.github.com/":
    npmAlwaysAuth: true
    npmAuthToken: "${MIDNIGHTCI_PACKAGES_READ}"
EOF
fi

# The SDK pins yarn 4.10.3 via packageManager + a committed .yarn/releases launcher; corepack (bundled
# with node) honours both, so no system yarn is needed.
corepack enable >/dev/null 2>&1 || true

echo "==> yarn install in ${COMPACT_JS_ROOT}"
( cd "$COMPACT_JS_ROOT" && corepack yarn install )

# Build the runtime via nix. The SDK's `build:submodule-runtime` script runs a bare `nix develop` on
# compact-submodule, which nix treats as a git working tree — but the COPY'd submodule has no usable
# `.git` in the build context, so that fails to resolve the submodule gitdir. Force a `path:` flake
# ref (the same trick +compactc-bundle uses for compactc) so nix copies the directory contents and
# ignores git. Needs nix on PATH (nixos/nix base image) + the IOG cache for zkir/onchain-runtime.
echo "==> Building compact-runtime via nix (path: ref; first build can be slow)"
nix develop "path:${SDK_DIR}/compact-submodule" --command bash -c "cd '${RUNTIME_DIR}' && yarn build"

# Build the three compact-js packages (turbo). The runtime dist now exists for their file: dep.
echo "==> Building compact-js packages (turbo) in ${COMPACT_JS_ROOT}"
( cd "$COMPACT_JS_ROOT" && corepack yarn build )

# pack_to <package-dir> <stable-name>: `npm pack` and move the produced tarball to a stable name.
pack_to() {
  local pkg_dir="$1" stable_name="$2" produced
  produced="$(cd "$pkg_dir" && npm pack --silent --pack-destination "$VENDOR_DIR")"
  mv -f "${VENDOR_DIR}/${produced}" "${VENDOR_DIR}/${stable_name}.tgz"
  echo "packed ${pkg_dir} -> ${VENDOR_DIR}/${stable_name}.tgz"
}

# Concrete runtime version to pin the compact-js packages against (read from the built package.json).
RUNTIME_VER="$(node -p "require('${RUNTIME_DIR}/package.json').version")"
echo "compact-runtime version: ${RUNTIME_VER}"
pack_to "$RUNTIME_DIR" "compact-runtime"

# Before packing, rewrite each package's `@midnight-ntwrk/compact-runtime` dependency from the in-repo
# `file:../../compact-submodule/runtime` link to the concrete built version, so the published tarball
# declares a real version that our vendored compact-runtime.tgz (declared directly by the variant)
# satisfies.
rewrite_runtime_dep() {
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
  ' "$1" "$2"
}

for entry in "${COMPACT_JS_PKGS[@]}"; do
  stable="${entry%%:*}"
  pkg_dir="${COMPACT_JS_ROOT}/${entry##*:}"
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
