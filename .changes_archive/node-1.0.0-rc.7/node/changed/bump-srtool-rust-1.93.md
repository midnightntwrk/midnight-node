#ci
# Bump srtool image to Rust 1.93.0 for konst 0.4.3 MSRV compatibility

`midnight-storage-core` 1.2.0-rc.3 pulled in `konst` 0.4.3, which raised its MSRV
to Rust 1.89. The srtool image was pinned to `paritytech/srtool:1.88.0-0.18.3`,
so deterministic runtime WASM builds failed at the cargo dependency check before
producing any artifact. Bumping both `srtool-build` and `srtool-info` targets to
`paritytech/srtool:1.93.0-0.18.4` restores the build. The toolchain change also
shifts the deterministic build baseline — downstream consumers verifying srtool
digests should re-anchor against the new image.

PR: https://github.com/midnightntwrk/midnight-node/pull/1508
