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

## Layout

Tests are grouped by topic across module files under `tests/`:

- `cnight.rs` — cNIGHT registration / deregistration / dust production lifecycle
- `governance.rs` — council, technical authority, federated ops, d-parameter, ariadne
- `rpc_abuse.rs` — DDoS and replay rejection at the RPC layer
- `contract_state.rs` — `contract_state` RPC behaviour
- `operational.rs` — manual / ignored operational tests
- `lib.rs` — shared statics, gates, and the global faucet manager (no tests)

All modules compile into a single `e2e_tests` binary so the global faucet and
pre-deploy gate are shared across the whole run.

To run only one group, filter by its module prefix:

```bash
cargo test-e2e-local cnight::            # all cNIGHT tests
cargo test-e2e-local governance::        # all governance tests
cargo test-e2e-local cnight::deregister  # everything starting with cnight::deregister
```

`cargo test`'s positional filter is a substring match against the full test
name (`module::fn_name`); the `::` suffix scopes the match to one module.

## Note on `cargo check`

The `[[test]]` entry in `Cargo.toml` sets `test = false`, so `cargo check
--tests` does **not** compile the integration target. To get real compile
errors / unused-import warnings from the e2e suite, use:

```bash
cargo test --test e2e_tests --no-run
```
