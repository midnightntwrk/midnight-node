# End to End Tests

> For the broader "how to test the Midnight node" guide — test levels, user
> flows, CI surface, release evidence — see
> [`docs/tests/how-to-test-node.md`](../../docs/tests/how-to-test-node.md).

These tests are not run by default when running `cargo test` in the workspace.

To execute these tests in CI, run `cargo test --test e2e_tests`
To execute these tests locally, run `cargo test --test e2e_tests --no-default-features --features local -- --no-capture` or simply using
alias: `cargo test-e2e-local`

To run test in parallel use `--test-threads N` argument, e.g.
`cargo test --test e2e_tests --no-default-features --features local -- --test-threads 6 --no-capture`

`--test-threads` should be large enough to let pre-deploy and deploy tests
run concurrently. Six is the historic recommendation for local-env.

**On Cardano Preview (`qanet` feature) thread count is load-bearing:** the
stability barrier amortises its ~3 h wait *only* when observation tests
run concurrently. With fewer threads than observation tests the wait is
paid per batch, which can blow past CI budgets. Set
`--test-threads >= 16` (we have 13 observation tests with headroom).
The nightly workflow sets this explicitly.

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

## Cardano stability barrier

Observation tests (those that assert the Midnight node saw a Cardano tx
they submitted) only succeed once the Cardano tx is *stable*, i.e. at
least `cardano_security_parameter` blocks behind the tip. On local-env
this parameter is ~5 blocks; on Cardano Preview it is 432 blocks
(~3 h). Tests can't observe before the follower processes a stable
block, so they wait.

The wait is handled by `MidnightClient::await_cnight_observations` —
each cNIGHT observation test calls it once (or twice, for tests that
need a `balance_before`/`balance_after` snapshot) with the Cardano
tx_ids it wants Midnight to observe. The helper subscribes to Midnight
blocks and returns when every requested tx_id has appeared in a
`process_tokens` extrinsic. Since Midnight only emits those extrinsics
for Cardano blocks past stability, the wait is implicit; no separate
Cardano-side polling helper is needed in the test body.

Progress is logged every ~30 s with the Cardano tip vs. target and the
running observed-count, so a multi-hour wait is legible:

```
await_cnight_observations: still waiting; midnight #1169115,
cardano: tip=4339244 target=4339214 (0 blocks to stability),
2/4 observed, 2 remaining
```

**Out of scope:** Cardano Preprod and mainnet (`k = 2160`, ~12 h wait)
exceed the 6 h GitHub Actions ceiling. No compile-time gate prevents
running there; just don't.

`create_hundred_registrations` is `#[cfg(any(feature = "local",
feature = "local-dev", feature = "local-ci"))]`-gated — it doesn't
exist when compiled with `--features qanet`.

## Toolkit wallet-cache warmup

Each cNIGHT observation test ends with a per-seed `dust_balance::execute`
that, with an empty `ledger_state_db`, replays the chain from genesis —
~1 h per seed on Preview. To avoid paying that N times serially, the
suite warms a shared `toolkit_cache/ledger_cache_db/` once via
`dust_balance::execute_many` while `await_cnight_observations` is
blocked on Cardano stability.

Mechanism:

1. Each cNIGHT test calls `register_test_seed(seed)` immediately after
   generating its random `WalletSeed`.
2. The first registration spawns a dedicated OS thread (its own
   current-thread tokio runtime) that polls a `WARMUP_QUIESCENCE`
   window (30 s of no new seed registrations) and then issues one
   `dust_balance::execute_many` covering every registered seed.
3. The warmup runs concurrently with the stability + observation
   barriers (the 3-h Preview wait), and writes wallet snapshots into
   `tests/e2e/toolkit_cache/ledger_cache_db/`.
4. Each test's later `dust_balance::execute(args)` uses that same
   `ledger_state_db` path; on cache hit it restores from the warm
   snapshot in seconds.

**`cargo test --test-threads >= N`** where N is the number of cNIGHT
observation tests. The warmup quiesces 30 s after the *latest* seed is
registered, so all tests must start in parallel for the warmup to
batch them in one pass. Serial execution would mean the warmup fires
on the first seed, then later tests miss the cache. A "warmup:
completed for K seed(s)" log line with K less than your test count is
the signal to raise `--test-threads`.

If the warmup task fails (e.g. node unreachable, fetch-cache backend
down), each test's `execute` falls back to its own genesis replay
without breaking. The failure surfaces as a `warmup: execute_many
failed: ...` log line.

## Logging

`tests/e2e/src/logger.rs` sets the default tracing filter to
`info,subxt=warn,jsonrpsee=warn,hyper=warn,reqwest=warn,midnight_ledger::semantics=warn`.

The `midnight_ledger::semantics=warn` entry is the one worth knowing
about. The upstream `midnight-ledger` crate emits an INFO line for
*every* privileged system transaction it applies — e.g.

```
INFO apply_system_tx: [privileged] DistributeNight outputs=[...
    OutputInstructionUnshielded { amount: 50000000000000, ... }, ...]
    kind=Reward supply_before=... locked_before=... tblock=... tx=...
```

That's the right level for the production node's audit trail. During
toolkit chain replay (warmup, `dust_balance::execute_many`,
`build_fork_aware_context_cached`) every historical privileged tx
re-emits it in a tight loop — pages of validator-wallet bootstrap
output between the fetch-progress and warmup-completion logs, with no
operational signal in it for the test. Demoting that target to WARN
keeps the test logs scannable; the toolkit's own info-level logs
(`fetch progress: ...`, `warmup: execute_many completed for N seed(s)
in ...`) still show replay progress.

Override at the command line with `E2E_LOG=...` for ad-hoc runs:

```fish
# Re-enable the privileged-tx audit log for one run:
E2E_LOG=info cargo test --features qanet ...

# Crank everything to debug:
E2E_LOG=debug,subxt=warn cargo test ...
```

`E2E_LOG` accepts the standard `tracing_subscriber::EnvFilter` syntax
(see [its docs](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html)).

## Toolkit fetch cache

The `midnight-node-toolkit` fetcher caches Cardano txs / wallet state
so reruns don't re-sync the chain from scratch. The cache backend is
selected by feature via `crate::fetch_cache_config()` in
`tests/e2e/tests/lib.rs`:

- **local-env** (`local`, `local-dev`, `local-ci`): `InMemory` —
  ephemeral RAM cache, no dependencies. Fine for short-lived local
  chains where syncing is cheap.
- **qanet**: `Postgres` at
  `postgresql://postgres:postgres@localhost:5433/toolkit_cache`.
  Persists across runs so the long-lived Preview chain doesn't need
  to be re-synced every time. (The URL is currently developer-local;
  the nightly workflow will need its own Postgres service container
  before this branch can run there end-to-end.)

To verify the Postgres path is wired correctly, run the smoke tests:

```bash
cargo test --release --test e2e_tests --no-default-features --features qanet \
    --ignored dust_balance_smoke -- --no-capture
cargo test --release --test e2e_tests --no-default-features --features qanet \
    --ignored dust_balance_smoke_many -- --no-capture
```

`dust_balance_smoke` (single seed) checks the Postgres `fetch_cache`
wiring. `dust_balance_smoke_many` (3 seeds) additionally exercises
`dust_balance::execute_many` and writes wallet snapshots into
`toolkit_cache/ledger_cache_db/`. Both fail fast with a connection
error if Postgres is down or the DB doesn't exist.

> **`--release` is required.** The toolkit's block replay is heavily
> compute-bound, and `cargo test` defaults to the dev profile (no
> optimisations, overflow checks on). Against qanet's ~1 M-block
> chain a debug build is roughly 50× slower than release — a full
> replay against the dev profile takes hours, with release it's
> ~15 min on a recent laptop. Same flag belongs in any nightly
> workflow that runs these tests.

> **Use the exact test name, and `--test-threads 1` if running more
> than one.** `cargo test ... dust_balance_smoke` is a substring match
> and will select both `dust_balance_smoke` and
> `dust_balance_smoke_many`. Running both concurrently inside the same
> e2e binary can deadlock / panic the ledger replay (the ledger
> crates carry process-global state that is not safe under concurrent
> contexts). Pass the exact name to filter to one test, or
> `--test-threads 1` to serialise.

## Indexer-side assertions (`indexer` feature)

The `c2m_bridge::*` tests double up as cross-checks against a running
`indexer-api`. Opt in with the `indexer` cargo feature:

```bash
cargo test --test e2e_tests --no-default-features --features local,indexer \
    c2m_bridge:: -- --test-threads=1 --nocapture
```

When the feature is on, each test runs the existing node-side checks and
then asserts on the corresponding indexer GraphQL surface:

| Test                                                            | Indexer surface(s) asserted                                       |
|-----------------------------------------------------------------|-------------------------------------------------------------------|
| `bridge_transfer_cnight_to_midnight_address`                    | `BridgeUserTransfer` row, `bridgeBalance` (pre/post claim), `BridgeClaimTransaction` row |
| `bridge_transfer_invalid_recipient_unlocks_to_treasury`         | `BridgeInvalidTransfer` row                                       |
| `unapproved_cardano_tx_makes_transfer_that_unlocks_to_treasury` | `BridgeUnapprovedTransfer` row                                    |
| `subminimal_transfers_accumulate_and_flush_on_threshold_breach` | `BridgeSubminimalFlushTransfer` row (asserts `amount` + `count`)  |

`bridgeBalance.balance` is asserted to be the **post-fee outstanding
claimable** (not the gross `deposited - claimed`). A regression that
flipped the semantics back to the pre-fee figure would fail the
pre-claim assertion in `bridge_transfer_cnight_to_midnight_address`.

The endpoint defaults to `http://127.0.0.1:8088/api/v3/graphql` (local-env's
indexer-api). Override with:

```bash
INDEXER_GRAPHQL_URL=http://my-indexer:8088/api/v3/graphql \
    cargo test --test e2e_tests --features local,indexer ...
```

Each test calls the indexer's `/ready` endpoint before running its
assertions, so a missing/unreachable indexer fails fast with a clear
error instead of timing out mid-test.

Without the `indexer` feature, the suite behaves exactly as before:
node-side assertions only, no indexer dependency, no extra HTTP traffic.

## Layout

Tests are grouped by topic across module files under `tests/`:

- `cnight.rs` — fast cNIGHT tests that don't need stability
  - `cnight/observation.rs` — cNIGHT registration / dust-production tests
    that need the stability barrier
- `governance.rs` — d-parameter, ariadne, federated-ops, council, tech-auth
  - `governance/observation.rs` — governance membership-reset observation
- `rpc_abuse.rs` — DDoS and replay rejection at the RPC layer
- `contract_state.rs` — `contract_state` RPC behaviour
- `operational.rs` — manual / ignored operational tests (`consolidate_faucet`,
  `valid_deploy_transaction_succeeds_via_rpc`)
- `lib.rs` — shared statics (deploy gate, stability barrier, faucet); no tests

All modules compile into a single `e2e_tests` binary so the global faucet
and barrier state are shared across the whole run.

To run only one group, filter by its module prefix:

```bash
cargo test-e2e-local cnight::                  # all cNIGHT (fast + observation)
cargo test-e2e-local cnight::observation::     # cNIGHT observation only
cargo test-e2e-local ::observation::           # all observation tests across modules
cargo test-e2e-local cnight::alice             # one specific fast test
cargo test-e2e-local governance::              # all governance
```

`cargo test`'s positional filter is a substring match against the full test
name (`module::fn_name`); the `::` suffix scopes the match to one module.

## Adding a new test family

When you introduce a new test module / user-flow group here (a new top-level
`tests/<topic>.rs` covering a distinct chain interaction), also add a
corresponding entry to **§2.2 "Main user flows we exercise"** in
[`docs/tests/how-to-test-node.md`](../../docs/tests/how-to-test-node.md). That
section is the SDET-facing inventory of what we actually exercise; keeping it
current as new modules land prevents the guide from drifting back into staleness.

## Note on `cargo check`

The `[[test]]` entry in `Cargo.toml` sets `test = false`, so `cargo check
--tests` does **not** compile the integration target. To get real compile
errors / unused-import warnings from the e2e suite, use:

```bash
cargo test --test e2e_tests --no-run
```
