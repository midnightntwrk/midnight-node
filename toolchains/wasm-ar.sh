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

# Wasm archiver: llvm-ar understands wasm object symbol tables. Homebrew LLVM on
# macOS, the distro's llvm-ar on Linux.
if [ -x /opt/homebrew/opt/llvm/bin/llvm-ar ]; then
  exec /opt/homebrew/opt/llvm/bin/llvm-ar "$@"
elif [ -x /usr/local/opt/llvm/bin/llvm-ar ]; then
  exec /usr/local/opt/llvm/bin/llvm-ar "$@"
else
  exec llvm-ar "$@"
fi
