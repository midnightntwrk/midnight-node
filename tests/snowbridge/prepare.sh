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
cd "$ROOT"
ART="$ROOT/artifacts/snowbridge"
mkdir -p "$ART/tools/compact" "$ART/tools/foundry"
bash "$ROOT/tests/snowbridge/prepare-sp1-contracts.sh"

prepare_checkout() {
    local url=$1 revision=$2 directory=$3 patch=$4
    if [[ ! -d "$directory/.git" ]]; then
        git clone --no-checkout "$url" "$directory"
        git -C "$directory" checkout --detach "$revision"
    fi
    [[ "$(git -C "$directory" rev-parse HEAD)" == "$revision" ]] || {
        echo "Unexpected revision in $directory; refusing to replace existing work" >&2; exit 1;
    }
    if git -C "$directory" apply --reverse --check "$patch" 2>/dev/null; then
        return
    fi
    git -C "$directory" apply --check "$patch"
    git -C "$directory" apply "$patch"
}
prepare_checkout https://github.com/Snowfork/snowbridge.git \
    caeea22dbee4d4da06266b205a61c89484c4e89f "$ART/upstream" "$ROOT/tests/snowbridge/patches/upstream.patch"
git -C "$ART/upstream" submodule update --init
prepare_checkout https://github.com/midnightntwrk/midnight-ledger.git \
    48a978d7fc4342e8f5536eb95f47139477e1a0e8 "$ART/ledger" "$ROOT/tests/snowbridge/patches/ledger.patch"
prepare_checkout https://github.com/midnightntwrk/midnight-ledger.git \
    da96e33d78ee9b7d3867b15c1037510d499cd909 "$ART/migration" "$ROOT/tests/snowbridge/patches/migration.patch"

# Downloads are pinned to the validated macOS/arm64 archives. Other platforms can supply tool paths.
if [[ "$(uname -s)/$(uname -m)" == Darwin/arm64 ]]; then
    if [[ ! -x "$ART/tools/compact/compactc" ]]; then
        curl -fL https://github.com/midnightntwrk/compact/releases/download/compactc-v0.34.0/compactc_v0.34.0_aarch64-darwin.zip -o "$ART/tools/compact.zip"
        python3 tests/snowbridge/check-tools.py compact
        unzip -q "$ART/tools/compact.zip" -d "$ART/tools/compact"
    fi
    if [[ ! -x "$ART/tools/foundry/forge" ]]; then
        curl -fL https://github.com/foundry-rs/foundry/releases/download/v1.8.6/foundry_v1.8.6_darwin_arm64.tar.gz -o "$ART/tools/foundry.tar.gz"
        python3 tests/snowbridge/check-tools.py foundry
        tar -xzf "$ART/tools/foundry.tar.gz" -C "$ART/tools/foundry"
    fi
fi
node -e 'if (Number(process.versions.node.split(".")[0]) < 22) throw new Error("Use Node 22+ (validated with 24.14.0)")'
npm ci --prefix tests/snowbridge --ignore-scripts --cache "$ART/npm-cache"
python3 - <<'PYTHON'
from pathlib import Path
root = Path.cwd()
compiled = root / "artifacts/snowbridge/compact"
compiled.mkdir(exist_ok=True)
(compiled / "package.json").write_text('{"type":"module"}\n')
link = compiled / "node_modules"
expected = root / "tests/snowbridge/node_modules"
if link.is_symlink():
    assert link.resolve() == expected, "unexpected generated dependency link"
elif not link.exists():
    link.symlink_to(expected, target_is_directory=True)
else:
    raise SystemExit("Unexpected generated node_modules; refusing to replace it")
PYTHON
