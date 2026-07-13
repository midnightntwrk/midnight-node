#!/usr/bin/env bash
# Wasm C compiler wrapper. Some crates in the runtime closure ship vendored C
# compiled for wasm (secp256k1-sys). cc-rs emits wasm codegen flags (-mcpu=mvp
# …) but does not map the `wasm32v1-none` rust target to a clang triple, so no
# --target reaches clang and it treats -mcpu=mvp as an x86 CPU. Force the wasm
# target here. Resolves a wasm-capable clang: Homebrew LLVM on macOS (Apple's
# /usr/bin/clang has no wasm backend), distro clang on Linux/CI.
# Mirrors ../midnight-node (bazel) toolchains/wasm32_cc.sh.
if [ -x /opt/homebrew/opt/llvm/bin/clang ]; then
  CLANG=/opt/homebrew/opt/llvm/bin/clang
elif [ -x /usr/local/opt/llvm/bin/clang ]; then
  CLANG=/usr/local/opt/llvm/bin/clang
else
  CLANG=clang
fi
# Capture + force-emit clang's stderr: buck's buildscript_run wrapper otherwise
# swallows it, leaving cc-rs failures with an empty diagnostic (undebuggable).
err="$("$CLANG" --target=wasm32-unknown-unknown "$@" 2>&1 1>/dev/null)"
rc=$?
if [ "$rc" -ne 0 ]; then
  printf 'wasm-cc.sh: %s exited %d\ncmd: %s %s\n%s\n' \
    "$CLANG" "$rc" "$CLANG" "$*" "$err" >&2
fi
exit "$rc"
