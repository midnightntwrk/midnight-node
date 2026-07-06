#node #local-env
# Add `local` network for the local-environment, bridge-funded at runtime

Add a dedicated `local` network with its own ledger identity (`mn_addr_local1…`),
decoupling the dockerized local-environment from `res/dev`, and fund it entirely over
the real cNIGHT↔NIGHT bridge — genesis ships **no** funded wallets, so the cross-chain
pool invariants hold from block 0 (fixes #1778).

Network:
- Config preset `res/cfg/local.toml` (real main chain; local-env Cardano/epoch params)
  and config namespace `res/local/`.
- `LocalNetwork` in `res/src/networks/definitions.rs`; `--chain local` resolves to it
  (`dev` still maps to `UndeployedNetwork`).
- Generated genesis `res/genesis/genesis_{state,block}_local.mn` — treasury/reserve/
  locked pools, no wallets. Earthfile `rebuild-genesis-state-local`
  (`--FUND_FAUCET_WALLETS=false`); the no-funding path also works without
  `cardano-tip.json`.
- `midnight-setup` sources chain-spec config from `res/local/`, derives the ICS/Reserve
  observation config from the **deployed** `ICS Forever` / `Reserve Forever` addresses,
  and anchors the bridge `initial_data_checkpoint` to the cNIGHT seeding tx (seeded ICS
  supply treated as pre-existing, not swept).

cNIGHT bridge funding — cardano-cli + toolkit only, no dedicated images, no governance
round:
- **Genesis seed** — a `mint-cnight-supply` service (between `contract-compiler` and
  `midnight-setup`) mints the full cNIGHT supply in one tx and splits it to mirror the
  Midnight pools: Reserve `C.R = M.R`, ICS `C.L = M.U` (inline unit datum, no bridge
  metadata), faucet `C.U = M.L`.
- **Dev wallet `0x..01`** — the same service then sends a c2m bridge transfer locking
  part of the circulating cNIGHT to ICS for it. The transfer spends the seeding tx's
  outputs, so it lands after the checkpoint and is observed; `midnight-setup`
  pre-approves its hash in the c2m-bridge genesis config (`approved_txs`, #1809).
  Post-genesis, `init-mnight-faucet` claims it (`claim-rewards --claim-kind
  cardano-bridge`, feeless/self-signed) and registers the wallet's DUST address so it
  can transact. Idempotent via a `runtime-values/mnight-faucet-ready` marker.

Per-wallet funding beyond `0x..01` is left to the e2e tests; the RPC tests that no
longer need funded genesis fixtures are revived on the bridge-funded wallet (#1792).

PR: https://github.com/midnightntwrk/midnight-node/pull/1796
Issue: https://github.com/midnightntwrk/midnight-node/issues/1778
Issue: https://github.com/midnightntwrk/midnight-node/issues/1792
