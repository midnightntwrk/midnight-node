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

PR: https://github.com/midnightntwrk/midnight-node/pull/2166
