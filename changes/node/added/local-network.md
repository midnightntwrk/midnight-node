#node
# Add `local` network for the local-environment + seed Cardano cNIGHT at genesis

Add a dedicated `local` network with its own ledger identity (addresses
`mn_addr_local1…`), decoupling the dockerized local-environment from `res/dev`, and seed
the Cardano side of the cNIGHT↔NIGHT bridge at genesis so the cross-chain pool invariants
hold (fixes #1778).

Includes:
- Config preset `res/cfg/local.toml` and config namespace `res/local/`
- `LocalNetwork` registered in `res/src/networks/definitions.rs`; the node
  `--chain local` now resolves to it (`dev` still maps to `UndeployedNetwork`)
- Generated genesis ledger state `res/genesis/genesis_{state,block}_local.mn`. The
  `local` genesis funds **no** faucet wallets; it carries the treasury/reserve/locked
  pools. `res/local` ics/reserve totals are set (reserve `5,000,000,000,873,988`, ics
  `2,200,000,000,000,000`) so the unfunded pools equal the mirrored Cardano amounts.
- Earthfile target `rebuild-genesis-state-local` (`--FUND_FAUCET_WALLETS=false`, added to
  `rebuild-all-genesis-states`); the no-funding genesis path also works without a
  `cardano-tip.json` (which mock-main-chain networks omit)
- `res/cfg/local.toml` follows the real main chain (no mock) and carries the
  local-environment Cardano/epoch params (60s epochs, security parameter 5)
- The local-environment `midnight-setup` entrypoint sources all chain-spec config from
  `res/local/` (`res/local-environment/` retired); builds the ICS/Reserve observation
  configs from the **deployed** `ICS Forever` / `Reserve Forever` addresses
  (`contracts-info.json`); and anchors the bridge `initial_data_checkpoint` to the cNIGHT
  seeding tx so the seeded ICS supply is treated as pre-existing locked supply (not swept).
- **cNIGHT genesis seeding (#1778)**: a `cnight-seeder` docker-compose service runs between
  `contract-compiler` and `midnight-setup` and mints the full cNIGHT supply
  (`S = 24e15` STARS) in one cardano-cli tx, distributing it to mirror the Midnight pools —
  Reserve Forever `C.R = M.R`, ICS Forever `C.L = M.U` (inline unit datum, **no** bridge
  metadata), faucet `C.U = M.L` — then records a `cnight-seeded` marker. `contract-compiler`
  exports the validator addresses + compiled cNIGHT policy for it. Mirrors
  `scripts/cnight-generates-dust/{receive_cnight,lock_to_ics}.sh`.
- `tests/e2e` contract-tx tests that relied on funded genesis fixtures are `#[ignore]`d;
  the no-funding RPC tests (contract-not-present / bad-address) remain active. Per-wallet
  funding is done by the e2e tests (which spend the faucet's circulating cNIGHT) and is out
  of scope here.

PR: https://github.com/midnightntwrk/midnight-node/pull/1796
Issue: https://github.com/midnightntwrk/midnight-node/issues/1778
Issue: https://github.com/midnightntwrk/midnight-node/issues/1792
