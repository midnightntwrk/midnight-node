# End to End Tests

These tests are not run by default when running `cargo test` in the workspace.

To execute these tests against local-env, run `cargo test --test e2e_tests -- --test-threads 8 --no-capture` or simply using
alias: `cargo test-e2e-local`

To run test in parallel use `--test-threads N` argument, e.g.
`cargo test --test e2e_tests -- --test-threads 4 --no-capture`
