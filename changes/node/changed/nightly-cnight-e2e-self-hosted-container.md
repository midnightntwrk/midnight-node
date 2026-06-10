#tests #ci
# Run nightly cNIGHT e2e job inside a container

The nightly `nightly-run-cnight-e2e-qanet` workflow failed on its
first run after moving to the Hetzner self-hosted runner:

```
sudo: a terminal is required to read the password; either use the
-S option to read from standard input or configure an askpass helper
```

The runner user is not a passwordless sudoer and has no tty, so
`sudo apt-get …` cannot prompt for a password. `ubuntu-latest`
configures passwordless sudo by default; self-hosted runners do not.

The job now runs inside a `rust:1.95-trixie` container — same Rust
version as `rust-toolchain.toml` and the Earthfile's `build-prepare`
stage — so apt installs happen as root with no sudo needed. The
container shares the host's network via Docker's default bridge, so
outbound connections still SNAT to the runner's static IP and the
toolkit-cache NLB allowlist keeps working.

The `dtolnay/rust-toolchain@stable` step is dropped (the image ships
rustup + Rust 1.95; the workspace's `rust-toolchain.toml` pulls any
missing components on first cargo invocation). The apt dep list adds
`pkg-config` on top of the existing `protobuf-compiler` and
`postgresql-client` — `libssl-dev`, `libpq-dev`, and `libsqlite3-dev`
are already in the image. `clang` is intentionally *not* installed:
the e2e test binary depends on `subxt` + `sqlx` + `redb` only, none of
which pull `rocksdb` or any other `bindgen`-using crate, so libclang
is not needed (the Earthfile installs it because it builds the full
node + runtime wasm, a heavier dep set than this job). Apt caches are
cleared after install so the cargo test build has disk headroom on
the runner — `target/` grows several GB during the release compile
(~8 min) that precedes the multi-hour Cardano stability waits.

A final `if: always()` step chowns the workspace back to the runner
user before the job exits. Without it, host-mode workflows landing on
the same self-hosted runner afterwards would fail to clean up the
root-owned `target/` and checkout artefacts the container leaves
behind.

PR: https://github.com/midnightntwrk/midnight-node/pull/1658
Issue: https://github.com/midnightntwrk/midnight-node/issues/1655
