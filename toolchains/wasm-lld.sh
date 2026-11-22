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

# Wasm cdylib link shim. buck2's system_cxx_toolchain infers linker_type from
# the host OS and appends host-only link flags (darwin `-install_name`, gnu
# driver `-Wl,...`) that rust-lld's `-flavor wasm` rejects: strip them, then run
# rust-lld. A wasm-aware cxx toolchain would make this unnecessary.
set -euo pipefail

# rust-lld ships in the toolchain's sysroot; $WASM_RUST_LLD overrides.
RUST_LLD="${WASM_RUST_LLD:-}"
if [ -z "$RUST_LLD" ]; then
  sysroot="$(rustc --print sysroot)"
  RUST_LLD="$(find "$sysroot/lib/rustlib" -name rust-lld -type f 2>/dev/null | head -1)"
fi
[ -x "$RUST_LLD" ] || { echo "wasm-lld.sh: rust-lld not found (sysroot=$(rustc --print sysroot 2>/dev/null))" >&2; exit 1; }
# rustc's wasm flavour emits raw lld flags, so any driver or darwin flag is one
# buck2 added: drop them from the args and from @argfiles.
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
