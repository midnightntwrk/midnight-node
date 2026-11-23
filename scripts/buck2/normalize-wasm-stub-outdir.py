#!/usr/bin/env python3

# This file is part of midnight-node.
# Copyright (C) Midnight Foundation
# SPDX-License-Identifier: Apache-2.0
# Licensed under the Apache License, Version 2.0 (the "License");
# You may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
# 	http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

"""Force every polkadot-sdk OUT_DIR in third-party/BUCK to its target's manifest dir.

A couple of polkadot-sdk crates (frame-storage-access-test-runtime, substrate-
test-runtime) can't run substrate-wasm-builder under buck, so a fixup overlays a
`WASM_BINARY = None` stub into the crate's manifest dir and sets OUT_DIR there,
to satisfy `include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"))`.

That OUT_DIR path carries reindeer's `polkadot-sdk-<hash>` git-fetch cell hash,
which changes on every polkadot-sdk bump. The hash lives in a static fixups.toml
`[env]`, so a bump moves CARGO_MANIFEST_DIR (reindeer-computed) but leaves OUT_DIR
pinned to the old hash -> include! points at a dir the overlay never populated
-> "couldn't read .../wasm_binary.rs". Re-pinning by hand drifts again on the
next bump. The invariant is OUT_DIR == CARGO_MANIFEST_DIR, so enforce it after
the third-party buckify.

Idempotent: a run that changes nothing exits 0 and reports 0 rewrites.
"""
import re, sys

BUCK = sys.argv[1] if len(sys.argv) > 1 else "third-party/BUCK"

# Within a reindeer target's `env = { ... }`, keys are sorted, so CARGO_MANIFEST_DIR
# (C < O) always precedes OUT_DIR. Track the manifest hash per target and rewrite
# any trailing polkadot-sdk OUT_DIR to match.
NEW_TARGET = re.compile(r"\bcargo\.rust_(?:library|binary)\(")
MANIFEST = re.compile(r'"CARGO_MANIFEST_DIR": "polkadot-sdk-([0-9a-f]+)/')
OUT_DIR = re.compile(r'("OUT_DIR": "polkadot-sdk-)([0-9a-f]+)(/)')

lines = open(BUCK).read().splitlines(keepends=True)
cur = None
rewrites = []
for i, line in enumerate(lines):
    if NEW_TARGET.search(line):
        cur = None
    m = MANIFEST.search(line)
    if m:
        cur = m.group(1)
        continue
    m = OUT_DIR.search(line)
    if m and cur and m.group(2) != cur:
        lines[i] = OUT_DIR.sub(rf"\g<1>{cur}\g<3>", line)
        rewrites.append((i + 1, m.group(2), cur))

if rewrites:
    open(BUCK, "w").writelines(lines)
for ln, old, new in rewrites:
    print(f"{BUCK}:{ln}: OUT_DIR hash {old} -> {new}")
print(f"normalize-wasm-stub-outdir: {len(rewrites)} rewrite(s)")
