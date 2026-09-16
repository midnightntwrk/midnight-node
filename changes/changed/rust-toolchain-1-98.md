#node #toolkit #runtime

# Update Rust toolchain to 1.98.1

Updates Rust to 1.98.1 across all components: the workspace
`rust-toolchain.toml`, the `subxt` and nightly cNIGHT e2e container base
images, and the srtool image used for deterministic runtime WASM builds
(now `shieldedtech/srtool:1.98.1-0.18.5`, as upstream `paritytech/srtool`
publishes no 1.98.x tag).

PR: https://github.com/midnightntwrk/midnight-node/pull/2166
