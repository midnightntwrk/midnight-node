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

# Helpers shared by the three chain-preparation jobs (patch-configs.sh,
# generate-genesis.sh, entrypoint.sh). Sourced, not executed.

check_json_validity() {
  local file="$1"
  if ! jq -e . "$file" > /dev/null 2>&1; then
    echo "Error: $file is invalid JSON."
    exit 1
  fi
}

# Big banner for the top-level phases of a job.
phase() {
  echo ""
  echo "############################################################"
  echo "##  $1"
  echo "############################################################"
}

# Smaller banner for each config section within a phase.
section() {
  echo ""
  echo "===== $1 ====="
}

# Patch a JSON file IN PLACE (jq can't read and write the same file, hence tmp+mv).
# The /res mount is the repo working tree, so every patched value lands there —
# drift between the static res/local configs and the deployed reality shows up as
# a git diff, easy to review and commit when regenerating genesis.
patch_json() {
  local file="$1"; shift
  jq "$@" "$file" > "$file.tmp"
  mv "$file.tmp" "$file"
}

# Print the serialization tag a genesis blob carries (e.g. `midnight:ledger-state[v18]`).
# The tag is version-bound: a node only deserializes the version its ledger crate
# speaks, so logging it makes an image/genesis mismatch obvious in the job output.
genesis_tag() {
  head -c 64 "$1" | tr -cd '[:print:]' | sed -n 's/^\(midnight:[^:]*\):.*/\1/p'
}
