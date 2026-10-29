#!/usr/bin/env bash
# This file is part of midnight-node.
# Copyright (C) Midnight Foundation
# SPDX-License-Identifier: Apache-2.0
# Licensed under the Apache License, Version 2.0 (the "License");
# You may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#	http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

set -euo pipefail
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
ART="$ROOT/artifacts/snowbridge"
GO=${GO:-$ART/tools/go/bin/go}
if [[ ! -x "$GO" ]]; then
    [[ "$(uname -s)-$(uname -m)" == Darwin-arm64 ]] || { echo "Set GO to a Go 1.25.3 binary" >&2; exit 1; }
    ARCHIVE="$ART/tools/go1.25.3.darwin-arm64.tar.gz"
    curl -fL --retry 3 https://go.dev/dl/go1.25.3.darwin-arm64.tar.gz -o "$ARCHIVE"
    echo "7c083e3d2c00debfeb2f77d9a4c00a1aac97113b89b9ccc42a90487af3437382  $ARCHIVE" | shasum -a 256 -c -
    tar -xzf "$ARCHIVE" -C "$ART/tools"
fi
[[ "$("$GO" version)" == *"go1.25.3 "* ]] || { echo "Expected Go 1.25.3" >&2; exit 1; }
[[ "$(git -C "$ART/upstream" rev-parse HEAD)" == caeea22dbee4d4da06266b205a61c89484c4e89f ]] || { echo "Run prepare.sh first" >&2; exit 1; }
export GOCACHE="$ART/go-cache" GOMODCACHE="$ART/go-modcache"
"$GO" -C "$ROOT/tests/snowbridge/proof-go" build -mod=readonly -o "$ART/tools/beacon-proof" .
