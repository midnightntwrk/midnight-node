# End to End Tests

These tests are not run by default when running `cargo test` in the workspace.

To execute these tests in CI, run `cargo test --test e2e_tests`
To execute these tests locally, run `cargo test --test e2e_tests --no-default-features --features local -- --no-capture` or simply using
alias: `cargo test-e2e-local`

To run test in parallel use `--test-threads N` argument, e.g.
`cargo test --test e2e_tests --no-default-features --features local -- --test-threads 6 --no-capture`

`--test-threads` must be `>= NUM_PRE_DEPLOY_TESTS + NUM_DEPLOY_TESTS` (currently 6) — see
the gate constants in `tests/e2e/tests/lib.rs`. Lower values can deadlock the deploy gate.

To run a single deploy test (e.g. `cargo test <name>`), set `E2E_SKIP_DEPLOY_GATE=1` to
bypass the pre-deploy gate. Without it, the deploy test will block forever waiting for
pre-deploy tests that aren't being run.
