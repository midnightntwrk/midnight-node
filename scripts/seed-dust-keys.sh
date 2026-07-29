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

# Seeds the local zk-params cache with Dust spend proving/verifying keys for
# ledger `static/version` 10 (the new ZKIR-v3 Dust circuit). See
# Batch-Verification-Notes.md, "Dust proving keys: use the bundled artifacts,
# not the published ones": the pinned midnight-ledger rev's
# `ledger/static/dust/spend.*.sha256` hashes disagree with what
# `srs.midnight.network/dust/10/spend.*` serves, and there is no plan to
# publish the new artifacts there — so `MidnightDataProvider` can never
# satisfy this fetch from the network.
#
# Unlike Zswap (scripts/seed-zswap-keys.sh), the new Dust prover key isn't
# byte-identical to any older cached version, and no binary copy of it is
# bundled anywhere — only its expected sha256 is. The only way to produce a
# matching file is to compile it: `zkir compile-many` runs deterministic key
# generation (halo2 `keygen_pk`) against the plaintext circuit source
# (`zkir-precompiles/dust/spend.zkir`, vendored in the pinned
# `midnight-ledger` git dependency) and the same public SRS params already
# used for Zswap. This is exactly what midnight-ledger's own nix flake does
# for its `local-params` package — and running it here reproduces the exact
# hashes `DUST_EXPECTED_FILES` expects.
#
# Respects the same cache-location override as the Rust data provider:
# $MIDNIGHT_PP / $XDG_CACHE_HOME/midnight/zk-params / $HOME/.cache/midnight/zk-params.

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/.." && pwd)"

cache_dir="${MIDNIGHT_PP:-${XDG_CACHE_HOME:-$HOME/.cache}/midnight/zk-params}"
version=10
dest_dir="${cache_dir}/dust/${version}"

# name:sha256, from the pinned midnight-ledger `ledger/static/dust/*.sha256` files.
files=(
  "spend.prover:6ecb69fc8e632cd42da78e4fcdd18a0ddba12633ec13c107c17c72d11815738e"
  "spend.verifier:d058526b0b42163b11b9cb47a3be676d5b59c23540a13b63e61e898a01da4aaa"
  "spend.bzkir:41264553ba6c3340a1c845eedc70a5e0eae61b6af5f7dd96d427319558817aa4"
)

verify_dest() {
  for entry in "${files[@]}"; do
    name="${entry%%:*}"
    hash="${entry##*:}"
    dest="${dest_dir}/${name}"
    [[ -f "${dest}" ]] || return 1
    echo "${hash}  ${dest}" | sha256sum -c - >/dev/null 2>&1 || return 1
  done
  return 0
}

if verify_dest; then
  echo "Dust keys already cached at ${dest_dir}"
  exit 0
fi

echo "Building the zkir-v3 compiler (compiles a halo2-based crate; may take a few minutes)..."
cargo build --release --bin zkir -p midnight-zkir-v3 --manifest-path "${repo_root}/Cargo.toml"

zkir_v3_manifest="$(cargo metadata --format-version=1 --manifest-path "${repo_root}/Cargo.toml" \
  | jq -r '.packages[] | select(.name=="midnight-zkir-v3") | .manifest_path')"
if [[ -z "${zkir_v3_manifest}" ]]; then
  echo "error: could not resolve the midnight-zkir-v3 dependency via cargo metadata" >&2
  exit 1
fi
precompiles_dir="$(dirname "$(dirname "${zkir_v3_manifest}")")/zkir-precompiles/dust"

work_dir="$(mktemp -d)"
trap 'rm -rf "${work_dir}"' EXIT
mkdir -p "${work_dir}/ir" "${work_dir}/keys"
cp "${precompiles_dir}/spend.zkir" "${work_dir}/ir/"

echo "Compiling Dust spend keys from ${precompiles_dir}/spend.zkir..."
MIDNIGHT_PP="${cache_dir}" "${repo_root}/target/release/zkir" compile-many "${work_dir}/ir" "${work_dir}/keys"

mkdir -p "${dest_dir}"
cp "${work_dir}/keys/spend.prover" "${dest_dir}/spend.prover"
cp "${work_dir}/keys/spend.verifier" "${dest_dir}/spend.verifier"
cp "${work_dir}/ir/spend.bzkir" "${dest_dir}/spend.bzkir"

if ! verify_dest; then
  echo "error: compiled Dust keys do not match the expected hashes (see DUST_EXPECTED_FILES upstream)" >&2
  exit 1
fi

echo "Dust keys cached at ${dest_dir}"
