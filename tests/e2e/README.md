# End to End Tests

These tests are not run by default when running `cargo test` in the workspace.

To execute these tests in CI, run `cargo test --test e2e_tests`
To execute these tests locally, run `cargo test --test e2e_tests --no-default-features --features local -- --no-capture` or simply using
alias: `cargo test-e2e-local`

To run test in parallel use `--test-threads N` argument, e.g.
`cargo test --test e2e_tests --no-default-features --features local -- --test-threads 6 --no-capture`

`--test-threads` should be large enough to let pre-deploy and deploy tests
run concurrently. Six is the historic recommendation (3 pre-deploy + 3
deploy tests in a full run); higher is fine.

## Pre-deploy / deploy gate

A few tests assert behaviour that depends on the test contract NOT being
deployed yet (RPC `ContractNotPresent`, DDoS rejection, etc.). They must
finish before any test that submits `DEPLOY_TX`.

The gate works by counter quiescence, not a hard-coded count, so it
adapts to subset runs that still include some pre-deploy tests:

- Each pre-deploy test holds a `PreDeployGuard` for its body (see
  `tests/e2e/tests/lib.rs`). Construction increments `PRE_DEPLOY_ENTERED`;
  drop increments `PRE_DEPLOY_COMPLETED`.
- `wait_before_deploying()` polls until `entered > 0`,
  `entered == completed`, and no counter change has happened for
  `PRE_DEPLOY_QUIESCENCE` (5 s).

`cargo test ... contract_state::` and `... rpc_abuse::` work without
any manual setup — both modules carry at least one pre-deploy test.

**Subset runs that select only deploy tests must opt out explicitly**
via `E2E_SKIP_DEPLOY_GATE=1`. The gate does not auto-open on a
timeout: there's no in-process way to distinguish "no pre-deploy tests
in this run" from "pre-deploy tests are scheduled but haven't started
yet" (e.g. under tight `--test-threads`), and opening on a timeout
would be unsound — a deploy test could race ahead and mutate chain
state before the pre-deploy assertions run.

```bash
E2E_SKIP_DEPLOY_GATE=1 cargo test-e2e-local valid_deploy_transaction
```

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
