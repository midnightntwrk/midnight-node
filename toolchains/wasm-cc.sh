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

# Wasm C compiler, for crates in the runtime closure that ship vendored C
# (secp256k1-sys). cc-rs emits wasm codegen flags (-mcpu=mvp ...) but does not
# map the `wasm32v1-none` rust target to a clang triple, so no --target reaches
# clang and it reads -mcpu=mvp as an x86 CPU: force the target here. Needs a
# wasm-capable clang: Homebrew LLVM on macOS (Apple's has no wasm backend), the
# distro's on Linux.
if [ -x /opt/homebrew/opt/llvm/bin/clang ]; then
  CLANG=/opt/homebrew/opt/llvm/bin/clang
elif [ -x /usr/local/opt/llvm/bin/clang ]; then
  CLANG=/usr/local/opt/llvm/bin/clang
elif [ -x /usr/bin/clang ]; then
  # Absolute: an action's env may carry no PATH.
  CLANG=/usr/bin/clang
else
  CLANG=clang
fi
# -Qunused-arguments: buck2's cc shim passes --ld-path to every invocation,
# -c compiles included, and cc-rs builds with -Werror=unused-command-line-argument.
# clang's stderr is captured and re-emitted because buck2's buildscript wrapper
# swallows it, which leaves a cc-rs failure with no diagnostic.
err="$("$CLANG" --target=wasm32-unknown-unknown -Qunused-arguments "$@" 2>&1 1>/dev/null)"
rc=$?
if [ "$rc" -ne 0 ]; then
  printf 'wasm-cc.sh: %s exited %d\ncmd: %s %s\n%s\n' \
    "$CLANG" "$rc" "$CLANG" "$*" "$err" >&2
fi
exit "$rc"
