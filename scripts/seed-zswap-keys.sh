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

# Seeds the Midnight zk-params cache with Zswap proving/verifying keys under
# both ledger `static/version` 9 and 10, so `MidnightDataProvider` (used by the
# toolkit's local prover, `ledger/helpers/src/versions/common/proving.rs`)
# always finds a locally cached, hash-verified copy and never has to reach
# https://srs.midnight.network at runtime.
#
# Version 10 was introduced only for the new ZKIR-v3 Dust artifacts (see the
# "chore(deps): bump ledger batch-verification pins..." commit); the Zswap
# files themselves are byte-identical between 9 and 10 upstream (verified by
# sha256 against the pinned `midnight-ledger` rev's `zswap/static/*.sha256`).
# So we fetch 9 once and copy it to 10 rather than fetching twice.
#
# Respects the same cache-location and source overrides as the Rust data
# provider: $MIDNIGHT_PP / $XDG_CACHE_HOME/midnight/zk-params /
# $HOME/.cache/midnight/zk-params, and $MIDNIGHT_PARAM_SOURCE.

set -euo pipefail

cache_dir="${MIDNIGHT_PP:-${XDG_CACHE_HOME:-$HOME/.cache}/midnight/zk-params}"
param_source="${MIDNIGHT_PARAM_SOURCE:-https://srs.midnight.network}"
source_version=9
target_versions=(9 10)

# name:sha256, from the pinned midnight-ledger `zswap/static/*.sha256` files.
files=(
  "spend.prover:19d234b5c68b7212ad6b0ec9334a95594748154128f3704eb576bcc843cc5c45"
  "spend.verifier:544554effd7ae9fb9063be52a9ec2a986756301071fcd97bb4598fb45a335658"
  "spend.bzkir:7cb5bbcf67cb212a3336fb439a77e8f32f0aa8a56185c8e1247d6cbfc7300205"
  "output.prover:d992b04f13c3fd432f55fb8bfe6466d87bc181f1a2acf233ec228030bbdd4ed8"
  "output.verifier:72e8074856f2f5c504ade25a86a2b8902c64aeb9497c4c8e6b26dea842a0ab08"
  "output.bzkir:91dc8b401dd8385e8d29eaac018c70b578505f48c7952452ef319bc397fa1f1b"
  "sign.prover:fe7268dd2bdd107f862f881ac3c5bc71a6df77ce80bfed51cf71514648e660e0"
  "sign.verifier:e39a727caa0de167e6dd6122a9e3b758fecf48f9093ab77c0309648de8ce07e1"
  "sign.bzkir:37ea2094516e145a738126307cf92bd293f7cb524b1ccd49fa6f3225a9ec3a50"
)

source_dir="${cache_dir}/zswap/${source_version}"
mkdir -p "${source_dir}"

for entry in "${files[@]}"; do
  name="${entry%%:*}"
  hash="${entry##*:}"
  dest="${source_dir}/${name}"
  if [[ -f "${dest}" ]] && echo "${hash}  ${dest}" | sha256sum -c - >/dev/null 2>&1; then
    continue
  fi
  echo "Fetching zswap/${source_version}/${name} from ${param_source}..."
  curl -fsSL "${param_source}/zswap/${source_version}/${name}" -o "${dest}"
  echo "${hash}  ${dest}" | sha256sum -c -
done

for version in "${target_versions[@]}"; do
  [[ "${version}" == "${source_version}" ]] && continue
  version_dir="${cache_dir}/zswap/${version}"
  mkdir -p "${version_dir}"
  for entry in "${files[@]}"; do
    name="${entry%%:*}"
    dest="${version_dir}/${name}"
    [[ -f "${dest}" ]] && continue
    cp "${source_dir}/${name}" "${dest}"
  done
done

joined_versions="$(IFS=,; echo "${target_versions[*]}")"
echo "Zswap keys cached at ${cache_dir}/zswap/{${joined_versions}}"
