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

# Generates baseline + malformed chainspecs into $OUT_DIR. Runs inside the
# midnight-node image (jq + the node binary are already on PATH there).

set -euo pipefail

OUT_DIR="${OUT_DIR:-/specs}"
BASE="$OUT_DIR/baseline.json"

mkdir -p "$OUT_DIR"

/midnight-node build-spec --chain dev --disable-default-bootnode > "$BASE"

# Sanity: baseline must have a non-empty genesis_extrinsics array.
count=$(jq '.properties.genesis_extrinsics | length' "$BASE")
if [ "$count" -lt 4 ]; then
    echo "baseline genesis_extrinsics has only $count entries; need >= 4 for mutations" >&2
    exit 1
fi

# 1. Non-string element at index 3 (middle position).
jq '.properties.genesis_extrinsics[3] = 123' "$BASE" > "$OUT_DIR/nonstring.json"

# 2. String present but not valid hex.
jq '.properties.genesis_extrinsics[3] = "xxxabc"' "$BASE" > "$OUT_DIR/bad-hex.json"

# 3. Audit's exact example.
jq '.properties.genesis_extrinsics = ["01020304", 123, "a1b2c3d4", "xxxabc"]' \
   "$BASE" > "$OUT_DIR/audit-example.json"
