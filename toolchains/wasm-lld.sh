#!/usr/bin/env bash
# Wasm cdylib link shim. buck's system_cxx_toolchain infers linker_type from the
# host OS and appends host-only link flags (darwin `-install_name`, gnu driver
# `-Wl,…`) that rust-lld's `-flavor wasm` rejects. Strip them, then invoke
# rust-lld. STOPGAP until a wasm-aware cxx toolchain (linker_type="gnu") exists.
set -euo pipefail

# Locate rust-lld under the active rust toolchain's sysroot — portable across
# machines/OSes/arches (macOS-arm64 dev box AND x86_64 Linux CI). Overridable
# via $WASM_RUST_LLD for exotic setups. rust-lld ships with the toolchain.
RUST_LLD="${WASM_RUST_LLD:-}"
if [ -z "$RUST_LLD" ]; then
  sysroot="$(rustc --print sysroot)"
  RUST_LLD="$(find "$sysroot/lib/rustlib" -name rust-lld -type f 2>/dev/null | head -1)"
fi
[ -x "$RUST_LLD" ] || { echo "wasm-lld.sh: rust-lld not found (sysroot=$(rustc --print sysroot 2>/dev/null))" >&2; exit 1; }
# buck's system_cxx_toolchain is darwin (host) and appends clang-driver + darwin
# link flags. rustc's wasm flavour emits raw lld flags (`-flavor wasm --export`),
# so every driver/darwin flag here is buck-added noise rust-lld can't parse; drop
# them (from both direct args and @argfiles) and keep the raw lld flags + objects.
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# true if a single token is a darwin/driver flag rust-lld can't take.
is_darwin_flag() {
  case "$1" in
    -fuse-ld=*|-Wl,*|-dynamiclib|-dylib|-bundle) return 0 ;;
    *) return 1 ;;
  esac
}

filter_argfile() {  # $1 = argfile path -> prints a filtered copy path
  local out; out="$TMP/af.$RANDOM"
  local skip=0 line
  : > "$out"
  while IFS= read -r line; do
    if [ "$skip" = 1 ]; then skip=0; continue; fi
    case "$line" in
      -install_name|-arch|-macos_version_min|-platform_version) skip=1; continue ;;
    esac
    is_darwin_flag "$line" && continue
    printf '%s\n' "$line" >> "$out"
  done < "$1"
  printf '%s' "$out"
}

args=()
skip=0
for a in "$@"; do
  if [ "$skip" = 1 ]; then skip=0; continue; fi
  case "$a" in
    -install_name|-arch|-macos_version_min|-platform_version) skip=1; continue ;;
    @*) args+=("@$(filter_argfile "${a#@}")"); continue ;;
  esac
  is_darwin_flag "$a" && continue
  args+=("$a")
done
exec "$RUST_LLD" "${args[@]}"
