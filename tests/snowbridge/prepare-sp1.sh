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

set -euo pipefail
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
ART="$ROOT/artifacts/snowbridge"
TOOLS="$ART/tools/sp1"
[[ $(uname -sm) == 'Darwin arm64' ]] || { echo 'This installer is validated on macOS arm64. Supply an SP1 6.10.0 toolchain on other hosts.' >&2; exit 1; }
mkdir -p "$TOOLS/rust"
fetch() {
    local url=$1 dest=$2 digest=$3
    if [[ ! -f "$dest" ]]; then curl -fL --retry 3 "$url" -o "$dest"; fi
    echo "$digest  $dest" | shasum -a 256 -c -
}
fetch 'https://github.com/succinctlabs/sp1/releases/download/v6.10.0/cargo_prove_v6.10.0_darwin_arm64.tar.gz' "$TOOLS/cli.tar.gz" 250cb4ff9f62f849995e939b3b501d394159fe2dedd9caaf9e73dba19004a462
fetch 'https://github.com/succinctlabs/rust/releases/download/succinct-1.96.0-64bit-v3/rust-toolchain-aarch64-apple-darwin.tar.gz' "$TOOLS/rust.tar.gz" a8e9e9695af87d8d1277e7febb8ca238004d634ff489158b32fe04e71fea63b3
[[ -x "$TOOLS/cargo-prove" ]] || tar -xzf "$TOOLS/cli.tar.gz" -C "$TOOLS"
[[ -x "$TOOLS/rust/bin/rustc" ]] || tar -xzf "$TOOLS/rust.tar.gz" -C "$TOOLS/rust"
[[ $("$TOOLS/cargo-prove" prove --version) == 'cargo-prove sp1 (d83ed91 '* ]] || { echo 'Unexpected SP1 compiler revision' >&2; exit 1; }
[[ $("$TOOLS/rust/bin/rustc" --version) == 'rustc 1.96.0-dev' ]]
rustup toolchain link succinct "$TOOLS/rust"
bash "$ROOT/tests/snowbridge/prepare-sp1-contracts.sh"
export PATH="$TOOLS:$ART/tools/go/bin:$PATH"
export CARGO_TARGET_DIR="$ART/sp1-target"
cargo build --locked --release --manifest-path "$ROOT/tests/snowbridge/sp1/Cargo.toml" -p midnight-sp1-prover
KEY=$(mktemp -d "$ART/sp1-key-build-XXXXXX")
"$CARGO_TARGET_DIR/release/midnight-sp1-prover" --out "$KEY/key"
cp "$KEY/key/proof.json" "$ART/sp1-program.json"
shasum -a 256 "$TOOLS/cargo-prove" "$TOOLS/rust/bin/rustc" "$CARGO_TARGET_DIR/release/midnight-sp1-prover" "$ROOT/tests/snowbridge/sp1/Cargo.lock" > "$ART/sp1-build.sha256"
echo "SP1 key: $ART/sp1-program.json"
