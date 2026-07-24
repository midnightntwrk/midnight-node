#tests
# Split e2e test suite into per-topic module files

Break the 3300-line `tests/e2e/tests/lib.rs` into topic-scoped module
files (`cnight.rs`, `governance.rs`, `rpc_abuse.rs`, `contract_state.rs`,
`operational.rs`) while keeping everything in a single `e2e_tests`
binary so the shared faucet, pre-deploy gate, and deploy mutex are still
process-global. `lib.rs` now holds only the shared statics and helpers
(`pub(crate)` so submodules can call them) plus `mod` declarations.
Tests can now be run per group via `cargo test ... cnight::`, etc.

PR: https://github.com/midnightntwrk/midnight-node/pull/1565
Issue: https://github.com/shieldedtech/shielded-qa/issues/49
