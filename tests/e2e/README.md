# Integration Tests

These tests are not run by default when running `cargo test` in the workspace.

To execute these tests against local-env, run `cargo test --test e2e_tests --features local`
To execute these tests in ci, run `cargo test --test e2e_tests --features local-ci` or simply
`cargo test --test e2e_tests` as `local-ci` is set as default.
