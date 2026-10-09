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

# Fail a release build that would ship the runtime's `fork-transition` feature.
#
# That feature relaxes seal, inherent and execution checks at one block height
# (see runtime/src/fork_transition.rs) and must never reach a released node or
# runtime. Cargo feature unification can turn it on for a whole build when any
# one crate in it asks for it, so release builds check both ends:
#
#   features [CARGO_TREE_ARGS...]
#       Before building: fail if `fork-transition` is in the cargo feature
#       graph for the given package selection and features, e.g.
#       `features --workspace --features runtime-benchmarks`.
#
#   wasm FILE...
#       After building: fail if any runtime wasm carries the fork-transition
#       `impl_name`. Pass uncompressed wasm (`*.wasm`, `*.compact.wasm`);
#       `*.compact.compressed.wasm` is skipped, since zstd hides the string.
#
# Keep FORK_IMPL_NAME in sync with FORK_TRANSITION_IMPL_NAME in
# runtime/src/lib.rs.

set -euo pipefail

FORK_IMPL_NAME="midnight-fork-transition-UNSAFE"

fail() {
	echo "::error::$*" >&2
	echo "error: $*" >&2
	echo "The runtime's fork-transition feature must never be released. Find what enables it with:" >&2
	echo "  cargo tree -e features -i midnight-node-runtime" >&2
	exit 1
}

mode="${1:-}"
shift || true

case "$mode" in
features)
	tree="$(cargo tree --locked -e features -i midnight-node-runtime --prefix none "$@")"
	if grep -q 'midnight-node-runtime feature "fork-transition"' <<<"$tree"; then
		fail "fork-transition is enabled in this build's feature graph (cargo tree $*)"
	fi
	echo "ok: fork-transition is not in the feature graph (cargo tree $*)"
	;;
wasm)
	[ "$#" -gt 0 ] || fail "wasm: no files given"
	checked=0
	for file in "$@"; do
		case "$file" in
		*.compressed.wasm) continue ;;
		esac
		[ -f "$file" ] || fail "wasm: no such file: $file"
		if grep -aqF "$FORK_IMPL_NAME" "$file"; then
			fail "$file is a fork-transition runtime (impl_name $FORK_IMPL_NAME)"
		fi
		checked=$((checked + 1))
	done
	[ "$checked" -gt 0 ] || fail "wasm: only compressed wasm given; pass an uncompressed build too"
	echo "ok: $checked runtime wasm file(s) are not fork-transition builds"
	;;
*)
	echo "usage: $0 features [CARGO_TREE_ARGS...] | wasm FILE..." >&2
	exit 2
	;;
esac
