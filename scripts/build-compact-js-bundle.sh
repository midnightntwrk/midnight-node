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

# Builds the four GitHub-Packages-only tarballs the `compact-0.33.0` toolkit-js variant consumes via
# `file:` references (see util/toolkit-js/compact-0.33.0/vendor/README.md). They are produced from
# the pinned `midnight-sdk` submodule:
#
#   compact-js.tgz          ┐
#   compact-js-command.tgz  ├─ built from midnight-sdk/compact-js (yarn install + the SDK's build-utils
#   compact-js-node.tgz     ┘  `package` step) and packed from each package's processed `dist/`
#   compact-runtime.tgz     ── `npm pack` of the published @midnight-ntwrk/compact-runtime@<version>
#                              (the exact version compact-js depends on) from GitHub Packages
#
# IMPORTANT: pack the compact-js packages from their build-utils `dist/` output (the SDK's `package`
# script does `cd dist && npm pack`), NOT a raw `npm pack` of the source dir — the source `package.json`
# `exports` point at `./src/*.ts`, whose emitted `.d.ts` don't line up with `@effect/cli`'s `Command`
# type in a consumer (a `[TypeId]` mismatch). The `dist/` layout matches the published packages.
#
# Everything else in the closure resolves from public npm and is NOT vendored: @midnightntwrk/ledger-v9,
# @midnightntwrk/onchain-runtime-v4, @midnight-ntwrk/platform-js, @midnight-ntwrk/wallet-sdk-address-format,
# @midnight-ntwrk/ledger-v8, and the @effect/* packages.
#
# No nix and no compact-submodule: at this SDK revision compact-runtime is a published dev build, not an
# in-tree `file:` runtime. This needs only node (+ corepack) and a GitHub Packages read token. The
# consume path (`npm ci` in util/toolkit-js) needs neither. Env knobs:
#   MIDNIGHTCI_PACKAGES_READ  GitHub Packages read token (required: SDK install + compact-runtime pack)
#   SDK_DIR                   path to the midnight-sdk checkout  (default: <repo>/midnight-sdk)
#   VENDOR_DIR                where to write the four tarballs    (default: variant vendor/ dir)

set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

SDK_DIR="${SDK_DIR:-${repo_root}/midnight-sdk}"
VENDOR_DIR="${VENDOR_DIR:-${repo_root}/util/toolkit-js/compact-0.33.0/vendor}"
COMPACT_JS_ROOT="${SDK_DIR}/compact-js"
COMPACT_JS_PKGS=(compact-js compact-js-command compact-js-node)

if [ ! -d "$COMPACT_JS_ROOT" ]; then
  echo "error: midnight-sdk compact-js workspace not found at ${COMPACT_JS_ROOT}." >&2
  echo "       Run: git submodule update --init midnight-sdk" >&2
  exit 1
fi
for tool in node npm; do
  command -v "$tool" >/dev/null 2>&1 || { echo "error: ${tool} is required." >&2; exit 1; }
done

mkdir -p "$VENDOR_DIR"

# --- GitHub Packages auth for the @midnight-ntwrk scope (SDK install + compact-runtime pack) ---------
# Never bundled into the produced tarballs; used only during this build.
if [ -z "${MIDNIGHTCI_PACKAGES_READ:-}" ]; then
  echo "warning: MIDNIGHTCI_PACKAGES_READ is unset; the SDK install and compact-runtime pack will fail." >&2
fi
# npm side (compact-runtime pack):
{
  echo "@midnight-ntwrk:registry=https://npm.pkg.github.com"
  echo "//npm.pkg.github.com/:_authToken=${MIDNIGHTCI_PACKAGES_READ:-}"
} >> "${HOME}/.npmrc"
# yarn side: the project .yarnrc.yml scopes @midnight-ntwrk to npm.pkg.github.com with npmAlwaysAuth but
# carries no token. Append a top-level npmRegistries block (a new key — no conflict with npmScopes).
# Guard on `npmAuthToken` (present only after our append), NOT on `npm.pkg.github.com` — the SDK's
# .yarnrc.yml already references that registry in npmScopes, so guarding on it would skip the token.
if [ -n "${MIDNIGHTCI_PACKAGES_READ:-}" ] && ! grep -q npmAuthToken "${COMPACT_JS_ROOT}/.yarnrc.yml"; then
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

echo "==> yarn install + package (build-utils) in ${COMPACT_JS_ROOT}"
# `package` = `turbo run package`, which depends on `build` (build-esm + build-utils pack-v3) and then
# runs each package's `package` script (`npm pack` from its processed dist/). Plain TS — no nix.
(
  cd "$COMPACT_JS_ROOT"
  corepack yarn install
  corepack yarn package
)

echo "==> Collecting the compact-js dist/ tarballs"
for pkg in "${COMPACT_JS_PKGS[@]}"; do
  produced="$(ls "${COMPACT_JS_ROOT}/${pkg}/dist/"*.tgz 2>/dev/null | head -n1)"
  [ -n "$produced" ] || { echo "error: no dist/ tarball produced for ${pkg}" >&2; exit 1; }
  cp -f "$produced" "${VENDOR_DIR}/${pkg}.tgz"
  echo "  ${pkg}.tgz  <- ${produced#"${SDK_DIR}/"}"
done

# compact-runtime: the published dev version compact-js depends on (read it, don't hardcode the hash).
RUNTIME_VER="$(node -p "require('${COMPACT_JS_ROOT}/compact-js/package.json').dependencies['@midnight-ntwrk/compact-runtime']")"
echo "==> Packing @midnight-ntwrk/compact-runtime@${RUNTIME_VER} from GitHub Packages"
produced="$(npm pack "@midnight-ntwrk/compact-runtime@${RUNTIME_VER}" --silent --pack-destination "$VENDOR_DIR")"
mv -f "${VENDOR_DIR}/${produced}" "${VENDOR_DIR}/compact-runtime.tgz"
echo "  compact-runtime.tgz  <- ${produced}"

echo ""
echo "Done. Vendored tarballs in ${VENDOR_DIR}:"
ls -la "${VENDOR_DIR}"/*.tgz
