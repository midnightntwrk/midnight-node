# End to End Tests

These tests are not run by default when running `cargo test` in the workspace.

To execute these tests in CI, run `cargo test --test e2e_tests`
To execute these tests locally, run `cargo test --test e2e_tests --no-default-features --features local -- --no-capture` or simply using
alias: `cargo test-e2e-local`

To run test in parallel use `--test-threads N` argument, e.g.
`cargo test --test e2e_tests --no-default-features --features local -- --test-threads 4 --no-capture`
