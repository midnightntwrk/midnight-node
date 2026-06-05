#node #toolkit
# Line-number panic backtraces, and a cargo-chef dependency cache

The `midnight-node` and `midnight-node-toolkit` release binaries now carry
`line-tables-only` debug info, so panic backtraces resolve to `file:line`
for our own crates (`RUST_BACKTRACE=1` still required at runtime).
Dependencies are built with no debug info
(`[profile.release.package."*"] debug = 0`) and the binaries' debug
sections are zlib-compressed after linking, keeping the cost modest:
`midnight-node` is ~331 MB, versus ~181 MB with no debug and ~1.9 GB if
debug were left on across the whole dependency graph. Backtrace frames
inside dependencies show addresses rather than line numbers — the
deliberate trade for size.

The Earthfile re-enables cargo-chef dependency caching as a
content-addressed image, named by a hash of the chef recipe, target
architecture, Rust version, and build flavor. A cache miss cooks
dependencies locally; a hit is pulled. Pushing to the shared registry is
gated behind `ALLOW_CACHE_PUSH`, which defaults to off so untrusted PR
builds cannot poison the cache — only trusted CI on `main` pushes. The
`check` (clippy) and `build` (release) flavors get separate images
because their cargo flags differ; `test`, `benchmarks`, and the
pallet-fixture tests share the `build` image. The previously test-only
`target-cpu=native` flag was removed: non-portable codegen under a fixed
fingerprint is unsound once artifacts are shared across machines.

PR: https://github.com/midnightntwrk/midnight-node/pull/1645
Issue:
