# Root cell targets.

# WASM_BINARY = None stub for the runtime. runtime/src/lib.rs does
# include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs")); the real file is
# emitted by substrate-wasm-builder (build.rs), which spawns cargo and can't
# run under buck. This stub lets the runtime lib + node binary compile and link
# with an empty embedded runtime. Building the actual on-chain wasm blob is a
# separate no_std crate-universe phase (see docs/buck2-notes.md). The runtime
# target sets OUT_DIR=$(location :runtime-wasm-stub) — see gen-first-party.py.
genrule(
    name = "runtime-wasm-stub",
    out = "out",
    cmd = "mkdir -p $OUT && printf 'pub const WASM_BINARY: Option<&[u8]> = None;\\n" +
          "pub const WASM_BINARY_BLOATY: Option<&[u8]> = None;\\n' > $OUT/wasm_binary.rs",
    visibility = ["PUBLIC"],
)

# ── On-chain WASM runtime blob (task #12) ──────────────────────────────────
# gen-first-party emits root//runtime:midnight-node-runtime-wasm-cdylib (a
# shared-lib rust_library, --cfg substrate_runtime, no_std wasm deps/features).
# Build it for wasm32 via a configured_alias, then wasm-opt + zstd-compress to
# the substrate sp-maybe-compressed-blob format, then a wasm_binary.rs that
# include_bytes!'s the blob — the real replacement for :runtime-wasm-stub.

# Build the runtime cdylib under the wasm32 platform. [cdylib] selects the
# shared-lib (`.wasm`) output rather than the default rmeta.
configured_alias(
    name = "runtime-wasm-cdylib-wasm32",
    actual = "root//runtime:midnight-node-runtime-wasm-cdylib[cdylib]",
    platform = "root//platforms:wasm32v1-none",
    visibility = ["PUBLIC"],
)

# wasm-opt compact (strip debug, lower sign-ext to MVP) then zstd-compress with
# the substrate magic prefix. wasm-opt is a host tool (must be on PATH).
genrule(
    name = "runtime-wasm",
    out = "midnight_node_runtime.compact.compressed.wasm",
    cmd = "COMPACT=$TMP/compact.wasm && " +
          "wasm-opt -O0 --mvp-features --strip-dwarf --signext-lowering " +
          "-o $COMPACT $(location :runtime-wasm-cdylib-wasm32) && " +
          "$(exe root//tools/wasm-compress:wasm-compress) $COMPACT $OUT",
    visibility = ["PUBLIC"],
)

# The real wasm_binary.rs: include_bytes! the compressed blob. Drop-in for
# :runtime-wasm-stub — point the runtime's OUT_DIR here to embed the live blob.
# The blob is copied INTO this OUT_DIR so the include_bytes! path is local
# (the runtime does include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs")), and
# rustc resolves include_bytes! relative to that file's dir).
genrule(
    name = "runtime-wasm-binary-rs",
    out = "out",
    cmd = "mkdir -p $OUT && cp $(location :runtime-wasm) $OUT/runtime.wasm && " +
          "printf 'pub const WASM_BINARY: Option<&[u8]> = Some(include_bytes!(\"runtime.wasm\"));\\n" +
          "pub const WASM_BINARY_BLOATY: Option<&[u8]> = Some(include_bytes!(\"runtime.wasm\"));\\n' " +
          "> $OUT/wasm_binary.rs",
    visibility = ["PUBLIC"],
)


# COMPACTC_VERSION exported so root-mirroring crates (util/toolkit) can place it
# in their srcs tree for include_str!("../../../COMPACTC_VERSION").
export_file(
    name = "COMPACTC_VERSION",
    src = "COMPACTC_VERSION",
    visibility = ["PUBLIC"],
)

