#!/usr/bin/env bash
# Wasm C compiler wrapper. Some crates in the runtime closure ship vendored C
# compiled for wasm (secp256k1-sys). cc-rs emits wasm codegen flags (-mcpu=mvp
# …) but does not map the `wasm32v1-none` rust target to a clang triple, so no
# --target reaches clang and it treats -mcpu=mvp as an x86 CPU. Force the wasm
# target here. Resolves a wasm-capable clang: Homebrew LLVM on macOS (Apple's
# /usr/bin/clang has no wasm backend), distro clang on Linux/CI.
# Mirrors ../midnight-node (bazel) toolchains/wasm32_cc.sh.
if [ -x /opt/homebrew/opt/llvm/bin/clang ]; then
  exec /opt/homebrew/opt/llvm/bin/clang --target=wasm32-unknown-unknown "$@"
elif [ -x /usr/local/opt/llvm/bin/clang ]; then
  exec /usr/local/opt/llvm/bin/clang --target=wasm32-unknown-unknown "$@"
else
  exec clang --target=wasm32-unknown-unknown "$@"
fi
