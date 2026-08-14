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

# Seeds the local zk-params cache with the Dust and Zswap proving/verifying keys
# for the pinned midnight-ledger's `static/version`, so `MidnightDataProvider`
# (used by the toolkit's local prover, see
# `ledger/helpers/src/versions/common/proving.rs`) always finds a locally
# cached, hash-verified copy.
#
# It has to compile them rather than fetch them: this branch's `static/version`
# (`10-dust-zswap-v3`) covers both the ZKIR-v3 Dust circuit and the ZKIR-v3
# Zswap recompile, and neither `srs.midnight.network/dust/<version>/*` nor
# `.../zswap/<version>/*` is published (both 403). See
# Batch-Verification-Notes.md, "Proving keys: compile them, they aren't
# published". `zkir compile-many` runs deterministic key generation (halo2
# `keygen_pk`) against the plaintext circuit sources vendored in the pinned
# `midnight-ledger` git dependency and the same public SRS params the data
# provider already downloads, reproducing the exact hashes the provider expects
# — this is what midnight-ledger's own nix flake does for its `local-params`
# package.
#
# Respects the same cache-location override as the Rust data provider:
# $MIDNIGHT_PP / $XDG_CACHE_HOME/midnight/zk-params / $HOME/.cache/midnight/zk-params.

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/.." && pwd)"

cache_dir="${MIDNIGHT_PP:-${XDG_CACHE_HOME:-$HOME/.cache}/midnight/zk-params}"

zkir_v3_manifest="$(cargo metadata --format-version=1 --manifest-path "${repo_root}/Cargo.toml" \
  | jq -r '.packages[] | select(.name=="midnight-zkir-v3") | .manifest_path')"
if [[ -z "${zkir_v3_manifest}" ]]; then
  echo "error: could not resolve the midnight-zkir-v3 dependency via cargo metadata" >&2
  exit 1
fi
ledger_root="$(dirname "$(dirname "${zkir_v3_manifest}")")"
version="$(cat "${ledger_root}/static/version")"

# <cache kind>:<dir in the pinned checkout holding the circuit sources and their expected hashes>
groups=(
  "dust:${ledger_root}/ledger/static/dust"
  "zswap:${ledger_root}/zswap/static"
)

# Every key the data provider looks up, hash-checked against the `.sha256` files
# shipped next to the circuit source.
verify_group() { # <circuit source dir> <destination dir>
  local src="$1" dest="$2" circuit name kind
  for circuit in "${src}"/*.zkir; do
    name="$(basename "${circuit}" .zkir)"
    for kind in prover verifier bzkir; do
      [[ -f "${dest}/${name}.${kind}" ]] || return 1
      echo "$(awk '{print $1}' "${src}/${name}.${kind}.sha256")  ${dest}/${name}.${kind}" \
        | sha256sum -c - >/dev/null 2>&1 || return 1
    done
  done
}

pending=()
for group in "${groups[@]}"; do
  kind="${group%%:*}"
  if verify_group "${group#*:}" "${cache_dir}/${kind}/${version}"; then
    echo "${kind} keys already cached at ${cache_dir}/${kind}/${version}"
  else
    pending+=("${group}")
  fi
done
[[ ${#pending[@]} -eq 0 ]] && exit 0

echo "Building the zkir-v3 compiler (compiles a halo2-based crate; may take a few minutes)..."
cargo build --release --bin zkir -p midnight-zkir-v3 --manifest-path "${repo_root}/Cargo.toml"

for group in "${pending[@]}"; do
  kind="${group%%:*}"
  src="${group#*:}"
  dest_dir="${cache_dir}/${kind}/${version}"

  work_dir="$(mktemp -d)"
  trap 'rm -rf "${work_dir}"' EXIT
  mkdir -p "${work_dir}/ir" "${work_dir}/keys" "${dest_dir}"
  cp "${src}"/*.zkir "${work_dir}/ir/"

  echo "Compiling ${kind} keys from ${src}..."
  MIDNIGHT_PP="${cache_dir}" "${repo_root}/target/release/zkir" compile-many \
    "${work_dir}/ir" "${work_dir}/keys"

  for circuit in "${work_dir}"/ir/*.zkir; do
    name="$(basename "${circuit}" .zkir)"
    cp "${work_dir}/keys/${name}.prover" "${work_dir}/keys/${name}.verifier" "${dest_dir}/"
    cp "${work_dir}/ir/${name}.bzkir" "${dest_dir}/"
  done

  rm -rf "${work_dir}"
  trap - EXIT

  if ! verify_group "${src}" "${dest_dir}"; then
    echo "error: compiled ${kind} keys do not match the hashes expected upstream" >&2
    exit 1
  fi
  echo "${kind} keys cached at ${dest_dir}"
done
