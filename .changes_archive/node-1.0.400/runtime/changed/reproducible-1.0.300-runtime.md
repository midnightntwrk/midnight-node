#runtime #reproducibility

# Rebuild the 1.0.300 runtime byte-identical to the one on chain

The ledger 8.1.2 bump ran a broad `cargo update`, which also moved `rand`
(0.10.0 -> 0.10.2) and `libc` (0.2.184 -> 0.2.189) inside the runtime's
dependency graph. The source was unchanged, but the new crate versions shift
every mangled symbol hash, so srtool here built a different wasm from the
runtime enacted on chain.

`Cargo.lock` now pins `rand 0.10.0`, `libc 0.2.184` and `atomic-write-file
0.3.0` (the only way to release `rand`). srtool now reproduces the enacted
runtime exactly: compressed wasm blake2
`0x39568eeb0802fa59d74ac0bc5c7ab62fc05c23ea947937458a4b011558516138`, the
`:code` hash on mainnet, preprod and preview.

`rand 0.10.0` carries RUSTSEC-2026-0097, ignored in `deny.toml`: it needs
rand's `log` feature, which nothing in the workspace enables.

PR: https://github.com/midnightntwrk/midnight-node/pull/2224
