#tests

# Gate local-only e2e tests behind the local features

The e2e crate defaults to `local-ci` and the qanet/devnet nightly job runs filtered to
`cnight::observation::`, so this never affects CI. But an unfiltered
`cargo test --no-default-features --features qanet` compiles and runs the local-env-only tests
(in `operational`, `contract_state`, `governance`, `c2m_bridge`, and the non-observation `cnight`
test), which call `ensure_dev_wallet_funded()` / the local faucet stack and hang ~180s.

cfg-gate the fully local-env-only modules (`contract_state`, `governance`, `c2m_bridge`) and the
individual local-only tests in `cnight` and `operational` behind
`any(feature = "local", feature = "local-dev", feature = "local-ci")`. `cnight` and `operational`
stay compiled under qanet/devnet because they own qanet-relevant tests: `cnight::observation::`
and `operational`'s `dust_balance_smoke` / `dust_balance_smoke_many` (the Postgres fetch-cache
smoke tests documented in the README). Under qanet the dev-wallet and deploy helpers in `lib.rs`
become unused, so a scoped `allow(dead_code, unused_imports)` (active only when no local feature
is set) keeps that build warning-clean without cfg-gating every helper.

PR: https://github.com/midnightntwrk/midnight-node/pull/2010
Issue: https://github.com/midnightntwrk/midnight-node/issues/1842
