#!/usr/bin/env python3
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
next bump (it has, twice). The invariant is simply OUT_DIR == CARGO_MANIFEST_DIR,
so enforce it mechanically here as the last step of the regen, after buckify.

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
