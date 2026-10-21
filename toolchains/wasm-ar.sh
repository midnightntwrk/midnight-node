#!/usr/bin/env bash
# Wasm archiver wrapper — llvm-ar understands wasm object symbol tables. Resolves
# Homebrew LLVM on macOS, distro llvm-ar on Linux/CI. Mirrors ../midnight-node
# (bazel) toolchains/wasm32_ar.sh.
if [ -x /opt/homebrew/opt/llvm/bin/llvm-ar ]; then
  exec /opt/homebrew/opt/llvm/bin/llvm-ar "$@"
elif [ -x /usr/local/opt/llvm/bin/llvm-ar ]; then
  exec /usr/local/opt/llvm/bin/llvm-ar "$@"
else
  exec llvm-ar "$@"
fi
