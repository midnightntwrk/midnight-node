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

# Prints the pinned Compact compiler version together with the 12-char git tree
# content hash of the `compact` submodule, in the form `<version>-<tree-hash>`
# (e.g. `0.31.0-6587676a9bb2`). This matches the format stored in the
# COMPACTC_VERSION file and the content-hash image tags described in
# docs/decisions/0004-tree-content-hash-image-tags.md.

set -euo pipefail

# Resolve the compact submodule directory relative to this script.
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/.." && pwd)"
compact_dir="${repo_root}/compact"

if [[ ! -d "${compact_dir}" ]]; then
  echo "error: compact submodule not found at ${compact_dir}; run 'git submodule update --init compact'" >&2
  exit 1
fi

# Extract the compiler version from the compact flake.nix. The relevant line is
# the compactc derivation's `version = "X.Y.Z";`, annotated with a comment
# referencing compiler/compiler-version.ss.
version="$(grep -oP 'version = "\K[0-9]+\.[0-9]+\.[0-9]+(?=";\s*#.*compiler-version)' "${compact_dir}/flake.nix" | head -n1)"

if [[ -z "${version}" ]]; then
  echo "error: could not determine compact version from ${compact_dir}/flake.nix" >&2
  exit 1
fi

# 12-char git tree content hash of the pinned submodule. Using the tree hash
# means the value tracks the actual source contents rather than commit metadata
# (see docs/decisions/0004-tree-content-hash-image-tags.md).
tree_hash="$(git -C "${compact_dir}" rev-parse HEAD^{tree} | cut -c1-12)"

echo "${version}-${tree_hash}"

