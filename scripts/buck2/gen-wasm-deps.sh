#!/usr/bin/env bash
# Regenerate wasm-deps/Cargo.toml — the no_std crate universe for the WASM
# runtime. Resolves midnight-node-runtime standalone with default-features=false
# filtered to wasm32v1-none (the exact no_std tree), then emits the manifest.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/src"; echo '// empty' > "$TMP/src/lib.rs"
{
  cat <<EOF
[package]
name = "wasm-resolve"
version = "0.0.0"
edition = "2024"
# resolver = "2" decouples host (proc-macro/build) feature unification from
# normal deps so the wasm/target feature set stays no_std (a bare [workspace]
# defaults to resolver "1" regardless of edition).
[workspace]
resolver = "2"
[lib]
path = "src/lib.rs"
[dependencies]
midnight-node-runtime = { path = "$ROOT/runtime", default-features = false }
EOF
  # append the workspace [patch] sections verbatim
  python3 - "$ROOT/Cargo.toml" <<'PY'
import tomllib, sys
root = tomllib.load(open(sys.argv[1], "rb"))
for reg, crates in root.get("patch", {}).items():
    print(f'\n[patch."{reg}"]')
    for n, s in crates.items():
        parts = [f'{k} = "{v}"' if isinstance(v, str) else f'{k} = {v!r}'
                 for k, v in s.items()]
        print(f'{n} = {{ {", ".join(parts)} }}')
PY
} > "$TMP/Cargo.toml"

( cd "$TMP" && cargo metadata --format-version 1 \
    --filter-platform wasm32v1-none -q ) > "$TMP/wasm-meta.json"

python3 "$ROOT/scripts/buck2/gen-wasm-deps.py" \
    "$TMP/wasm-meta.json" "$ROOT/wasm-deps/Cargo.toml" "$ROOT"
