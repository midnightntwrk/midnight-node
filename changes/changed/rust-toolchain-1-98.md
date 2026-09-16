#node #toolkit #runtime

# Update Rust toolchain to 1.98.1

Updates Rust to 1.98.1 across all components: the workspace
`rust-toolchain.toml`, the `subxt` and nightly cNIGHT e2e container base
images, and the srtool image used for deterministic runtime WASM builds
(now `midnightntwrk/srtool:1.98.1-0.18.5`, as upstream `paritytech/srtool`
publishes no 1.98.x tag).

Two build fixes were needed for 1.98.1: `ethnum` is bumped to 1.5.3 (1.5.2
transmuted `()` into `TryFromIntError`, which 1.98.1 no longer allows), and
`WASM_BUILD_RUSTFLAGS` restores `--allow-undefined` for the runtime WASM link
(1.98.1 dropped it from the `wasm32v1-none` target spec, which Substrate's
`sp_io` host-function imports rely on).

1.98.1 also ships new clippy lints (`useless_borrows_in_formatting`,
`unnecessary_get_then_check`) and flags some now-unused imports, so a handful of
call sites across the node, toolkit and the vendored `partner-chains` tree are
adjusted to keep `cargo clippy -D warnings` green.

PR: https://github.com/midnightntwrk/midnight-node/pull/2166
Issue: https://github.com/midnightntwrk/midnight-node/issues/2061
