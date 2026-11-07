#tests
# Restore cNIGHT observation coverage against Cardano Preview

The e2e cNIGHT tests were designed against local-env's near-instant
finalization. On Cardano Preview the Midnight mainchain follower only
processes blocks that are `k = 432` behind the tip (~3 h), so every
observation assertion timed out and the legacy branch
(`e2e-cngd-for-cardano-preview`) had to delete them outright.

Each cNIGHT observation test now calls a single
`MidnightClient::await_cnight_observations(&tx_ids, ...)` helper that:

- snapshots the current Cardano tip (after a 2-block advance, so
  just-submitted txs are guaranteed in a block at or below the
  target);
- polls Midnight's `nextCardanoPosition` watermark every 5 s,
  logging tip / target / blocks-remaining so a multi-hour wait is
  legible in CI;
- once the watermark crosses the target, binary-searches Midnight's
  block history for the boundary block and walks backwards decoding
  `process_tokens` extrinsics until every requested tx_id is matched;
- reconnects subxt's `OnlineClient` on `background task closed` /
  `restart required` and retries each past-scan RPC with a 30 s
  timeout, so transport hiccups don't strand a multi-hour wait.

The two tests that need a `balance_before` / `balance_after` snapshot
(`spend_cnight_producing_dust`,
`stop_dust_producing_after_deregistration_and_rotation`) submit their
second batch with `wait_for_block_spacing` between the two (≥ 5
Cardano + 5 Midnight blocks) and call `await_cnight_observations_at`
with an explicit target so the two windows don't collapse into one.

Register submissions are followed by `wait_for_tx_inclusion` so a
Cardano mempool / orphan flake fails the test in seconds, not after
the multi-hour stability window expires with the tx never observed.

To pay the ~3 h Preview wait once per run rather than once per seed,
the suite warms a shared toolkit `ledger_cache_db` while the
stability window is open: each test calls `register_test_seed` at
setup, the first registration spawns a 30 s-quiescence-then-fire
worker that issues one `dust_balance::execute_many` across every
registered seed. Each test's own `dust_balance::execute` then reads
the pre-warmed cache instead of replaying the chain from genesis.
If the warmup fails the per-test `execute` falls back to its own
replay; no test is blocked on the warmup.

The toolkit fetch cache itself is now backed by Postgres on qanet
(`fetch_cache_config()` in `tests/e2e/tests/lib.rs`). Local runs
default to an SSH-tunneled URL; CI overrides via the
`TOOLKIT_CACHE_DB_URL` env var wired up in
https://github.com/midnightntwrk/midnight-node/pull/1578. local-env
stays on `InMemory`.

Observation tests are moved into `cnight::observation::*` and
`governance::observation::*` submodules for filterability
(`cargo test ... ::observation::`). `create_hundred_registrations`
is `cfg`-gated to local-env only — 100 sequential txs would dominate
the stability window on Preview without exercising new code paths.

To make parallel observation tests safe against a real Cardano
network, the faucet is rewritten around a worker-UTXO pool so
parallel callers don't contend on the same on-chain input, and
whisky's `TxBuilder` now consumes real protocol parameters fetched
from Ogmios instead of mainnet defaults (the latter undercosted fees
by ~4× against Preview).

The nightly workflow runs against qanet on a self-hosted runner
(Hetzner; its static IP is on the toolkit-cache NLB allowlist),
fail-fast smoke-tests the cache DB with `psql ... -c 'SELECT 1;'`
before launching the ~4 h cargo job, and runs
`cargo test --release ... -- cnight::observation:: --test-threads 16`
so all observation tests share the single stability window and the
toolkit's compute-bound chain replay doesn't pay the ~50× dev-profile
tax. The schedule trigger runs from the default branch (main); manual
dispatch lets you pick any branch from the GitHub UI's "Use workflow
from" dropdown.

**Scope:** local-env and Cardano Preview (`qanet` feature). Preprod
and mainnet (`k = 2160`, ~12 h) exceed the GitHub Actions 6 h ceiling
and are out of scope.

PR: https://github.com/midnightntwrk/midnight-node/pull/1613
Issue: https://github.com/shieldedtech/shielded-qa/issues/50
