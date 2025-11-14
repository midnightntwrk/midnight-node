# Integration Tests

These tests are not run by default when running `cargo test` in the workspace.

To execute these tests in CI, run `cargo test --test e2e_tests`
To execute these tests locally, run `test --test e2e_tests --no-default-features --features local` or simply using
alias: `cargo test-e2e-local`
