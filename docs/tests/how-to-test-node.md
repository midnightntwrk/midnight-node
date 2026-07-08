# How to Test the Midnight Node

A practical guide for SDETs working on `midnight-node`.

---

## 1. Node — How it's connected and how it works

### 1.1 What is the Midnight Node?

- A **Substrate-based** blockchain that operates as a **Cardano Partner Chain**.
- Privacy-preserving: uses **zero-knowledge proofs** for shielded transactions.
- Consensus: **AURA** (6 s block time) + **GRANDPA** (finality) + **BEEFY** (bridge security).
- Reads from Cardano (mainchain) via **db-sync** through the **partner-chains** follower — the node consumes Cardano data but never writes back.

### 1.2 Layout (the parts you'll touch most)

```
/node/         Main binary, CLI, RPC server
/runtime/      Substrate runtime assembly
/pallets/      Custom pallets — e.g. midnight (core ledger), cnight-observation,
               c2m-bridge, federated-authority. See `pallets/` for the full list.
/primitives/   Shared runtime types/interfaces
/ledger/       Midnight ledger types & state
/res/cfg/      Per-network presets (e.g. dev / qanet / preview / preprod —
               see `res/cfg/` for the full set)
/util/toolkit/ Transaction generator + replay tool (you'll use this in tests)
/tests/e2e/    The Rust e2e suite — this is the main automation surface
/scripts/      Shell helpers, smoke checks, sync tests, genesis tooling
/local-environment/  TypeScript CLI to stand up local stacks and forks
```

### 1.3 How the pieces connect (mental model)

```
       External                       Midnight node (one binary)
   ─────────────────       ──────────────────────────────────────────────────────

   Tests / Toolkit ── 9944 (WS) ──►  RPC server
   (subxt, polkadot.js)                   │
                                          │  signed + unsigned extrinsics
                                          ▼
   Peer nodes      ── 30333     ──►  Tx pool / sync
                                          │
                                          ▼
   Cardano    ── db-sync ──►  Cardano follower ── inherents ──►  Block author + importer
   (Preview /                 (in-node service)                  (AURA · GRANDPA · BEEFY)
    Preprod /                                                           │
    Mainnet)                                                            ▼  executes each block against
                                                                  ╔════════════════════════════════╗
                                                                  ║   Runtime  (WASM, on-chain)    ║
                                                                  ║     • pallet-midnight          ║
                                                                  ║     • cnight-observation       ║
                                                                  ║     • c2m-bridge               ║
                                                                  ║     • federated-authority      ║
                                                                  ║     • version, throttle, …     ║
                                                                  ╚════════════════════════════════╝
                                                                        │
                                                                        ▼
                                                                  Storage (RocksDB) + events
```

> **Note on ogmios.** The node itself doesn't use ogmios — it follows Cardano via db-sync only.
> Ogmios is used by the **toolkit / cNIGHT-observation tests** to drive Cardano (see §3).

Three things to take away from the diagram:

1. **One binary, runtime inside.** Everything in the right column is *one* `midnight-node`
   process. The double-bordered box is the WASM runtime that lives on-chain — that's
   the piece a governance upgrade swaps; the surrounding Rust is the piece an
   `image-upgrade` swaps.
2. **Three ways data gets in.** RPC (extrinsics from tests/wallets), P2P (blocks
   from peers), and the Cardano follower (db-sync → follower → inherents). The first
   two land in the tx pool; the third is added by the block author itself.
3. **Where tests plug in.** The whole e2e suite talks to the leftmost arrow (RPC,
   WS, port 9944 — or 9933 inside local-env). The cNIGHT observation tests
   additionally drive Cardano via **ogmios-client** so they can later assert the
   follower-inserted inherents landed.

### 1.4 Terminology — the words you'll hit on day one

**Node vs runtime.** A Substrate chain has two halves, and they're upgraded differently:

- **Node** — the binary you run. Networking, RPC, database, block production.
  Bug fix → new image, operators redeploy.
  *Example:* cNIGHT observation followed the wrong Cardano tx-id range (#1365).
- **Runtime** — the chain's logic. Compiled to **WASM**, stored *on-chain*, executed
  by the node. Bug fix → new WASM via governance, no operator action.
  *Example:* throttle pallet got a `MaxTxs` per-account cap + `AccountUsage` storage
  migration (#1060).
- **Backward compatibility.** The node *hosts* the runtime, so it must be able to
  execute **every past runtime version** — a re-syncing node re-executes historical
  blocks against whatever runtime was on-chain at the time. Dropping support for an
  old runtime breaks re-sync and finality verification, so node releases keep older
  WASM execution paths alive.

**Pallet** — a chunk of runtime logic with its own storage, callable functions,
events, and errors. The runtime is just a list of pallets bolted together.
Everything Midnight-specific lives under `pallets/`.

**Extrinsic** — anything that goes into a block from *outside* the runtime.
Three kinds:

- **Signed transaction** — user-submitted, signed, pays fees. The closest match to
  a Cardano tx.
- **Unsigned transaction** — no Substrate signature. **Midnight's main user tx path
  is unsigned:** `send_mn_transaction` (`pallet-midnight`) carries a shielded
  payload that authorises itself via zk-proofs, so a Substrate signature would
  just be a weaker second layer. `pallet-midnight`'s unit tests assert the
  `validate_unsigned` / `pre_dispatch` path.
- **Inherent** — the block author inserts it; not a user tx. E.g. the timestamp,
  and the cNIGHT `process_tokens` payload several of our tests assert on.

**Metadata** — a machine-readable description of the runtime's external surface:
every pallet, every extrinsic with its argument types, every storage item, every
event, every error, every runtime API. Exposed by the node via the
`state_getMetadata` RPC; clients like **subxt** and **polkadot.js** use it to
encode/decode calls without hardcoding anything. The `metadata/` crate bundles
a compile-time snapshot so subxt generates typed bindings for the e2e suite
at build time — that's why pallet/extrinsic/storage changes need
`/bot rebuild-metadata` (or `earthly -P +rebuild-metadata`) to keep the
snapshot in sync, and why CI has a `metadata-check` job that fails the PR if
it isn't.

**Block content that isn't an extrinsic.** Block-header **digests** (AURA seal,
GRANDPA, BEEFY, plus `pallets/version` which writes the runtime version every
block). Events and storage mutations from extrinsic execution / `on_initialize` /
`on_finalize` land in state, not in the block body.

### 1.5 What we actually ship per release

A release isn't just one Docker image. Each tag (e.g. `node-1.0.0`) produces:

- **`midnight-node` Docker image** — the binary (node side).
- **`midnight-node-toolkit` Docker image** — the test/operator companion (transactions,
  dust balance, mappings).
- **Deterministic runtime WASM**, in three forms:
  - `midnight_node_runtime-<ver>.wasm`
  - `midnight_node_runtime-<ver>.compact.wasm`
  - `midnight_node_runtime-<ver>.compact.compressed.wasm` ← this is the one a
    governance runtime upgrade uploads on-chain.
- **`srtool-digest.json`** + **`SHA256SUMS-srtool`** — the digest and checksums
  produced by the deterministic build.
- **GitHub build-provenance attestations** on each WASM artifact — signed receipts
  (via Sigstore) tying the file's SHA256 to the workflow run and commit that
  produced it. Verify with
  `gh attestation verify <file> --repo midnightntwrk/midnight-node`.

The WASM is built with **srtool** (`paritytech/srtool`, pinned Rust + srtool
versions for determinism — see `Earthfile +srtool-build` and
`.github/workflows/srtool-build.yml`). The whole point is that anyone can
re-run the build from the tag and get **byte-identical** WASM, then verify
against `SHA256SUMS-srtool`. The same WASM can be re-used to deterministically
rebuild chainspecs (`DETERMINISTIC=true` path in the Earthfile).

Why this matters for SDETs:

- The WASM hash in `srtool-digest.json` is the single thing that defines the
  runtime behaviour you're testing — quoting it in test-evidence beats "I used
  rc.8" because it survives re-tags.
- A governance runtime upgrade in a fork rehearsal (`local-environment/
  governance-runtime-upgrade`) uploads exactly this `.compact.compressed.wasm`
  file. The artifact you download from the release is the artifact you test
  against.
- When something looks off ("did we actually build the right runtime?"), the
  attestation + sha256 chain is what you check.

Node image and toolkit image are versioned and tested together; the runtime
WASM is tagged independently of, but in lockstep with, the node tag.

---

## 2. How to test Node

### 2.1 Test levels

| Level                  | Where                                                 | What it covers                                                                 |
|------------------------|-------------------------------------------------------|--------------------------------------------------------------------------------|
| Unit                   | inline `#[test]` per pallet/crate                     | Pure logic, encoding/decoding, weights                                         |
| Pallet integration     | `pallets/<name>` mock runtime tests                   | Extrinsics against a mock runtime; storage migrations                          |
| Runtime / metadata     | `metadata/`, chainspec-validation, `+rebuild-metadata`| Runtime version & metadata snapshot is consistent                              |
| Toolkit / scripts E2E  | `scripts/tests/*.sh` (driven by `just …`)             | Docker-based smoke / contracts / mint / multi-dest / startup checks            |
| Stack E2E (local-env)  | `local-environment/` + `tests/e2e/` (`--features local`)| Full Cardano stack via Docker, then runs the e2e Rust suite                  |
| Network E2E (qanet)    | `tests/e2e/` (`--features qanet`), nightly job        | The same Rust suite, but against Cardano Preview-backed qanet                  |
| Release sign-off       | `docs/releases/<ver>/test-evidence/`                  | Manual evidence document derived from release notes + smoke/regression suite   |

### 2.2 Main user flows we exercise

These are the end-user journeys our automation asserts on. Most live in the Rust
e2e suite under `tests/e2e/tests/`; node lifecycle / sync is covered by shell
scripts under `scripts/`.

1. **Node lifecycle** *(shell scripts, not Rust e2e)*
   - Node starts cleanly, produces blocks, finalizes them (`scripts/tests/startup-dev-e2e.sh`, `startup-qanet-e2e.sh`).
   - Sync from genesis (`scripts/sync-test/{build-snapshot,run-sync}.sh`, `sync-with-qanet.sh`).
2. **cNIGHT → DUST bridge** (`cnight.rs`, `cnight/observation.rs`)
   - Register a Cardano address for DUST production.
   - DUST starts generating after stability + observation.
   - Deregister (full and partial / first-mapping).
   - Rotate / spend cNIGHT tokens, stop production accordingly.
   - Tokens owned **before** registration still produce after registration.
   - DDoS-shaped scenarios (`removing_excessive_registrations`, `create_hundred_registrations`).
3. **Cardano → Midnight bridge** (`c2m_bridge.rs`)
   - Transfer cNIGHT → Midnight recipient address (happy path, claim, post-fee balance).
   - Invalid recipient unlocks to treasury.
   - Unapproved Cardano tx unlocks to treasury.
   - Subminimal transfers accumulate and flush on threshold breach.
   - Opt-in indexer-side assertions (with `--features indexer`).
4. **Governance** (`governance.rs`, `governance/observation.rs`)
   - Federated-ops contract deployment shape.
   - D-parameter at historical block heights, matches config.
   - Ariadne parameters structure.
   - Permissioned candidates (Aiken-format), authority selection.
   - Membership reset observed from mainchain.
5. **Contract state RPC** (`contract_state.rs`)
   - Undeployed address ⇒ `ContractNotPresent`.
   - Unparseable address ⇒ explicit RPC error.
   - Historical vs. current block distinguish correctly.
6. **Pre-dispatch validation / DoS surface** (`pallet-midnight` unit tests)
   - Store for a never-deployed contract ⇒ `ContractNotPresent` at `pre_dispatch`.
   - Malformed and replayed txs rejected; validation doesn't mutate state.
7. **Operational / on-demand** (`operational.rs`, `--ignored`)
   - `valid_deploy_transaction_succeeds_via_rpc`
   - `consolidate_faucet`
   - `dust_balance_smoke`, `dust_balance_smoke_many` (verifies toolkit-cache wiring)

### 2.3 Two important e2e mechanics every test relies on

These are subtle — surface them in any onboarding session:

- **Pre-deploy / deploy gate** (`tests/e2e/tests/lib.rs::PreDeployGuard`)
  - Some tests assume the test contract is NOT deployed yet (`ContractNotPresent` cases).
  - The gate works by counter quiescence (no timeout), so subset runs work.
  - Subset runs that only pick deploy tests need `E2E_SKIP_DEPLOY_GATE=1`.
- **Cardano stability barrier** (`MidnightClient::await_cnight_observations`)
  - On local-env: **k = 5** (per `configurations/genesis/shelley/genesis.json`).
    On Preview: **k = 432 ≈ 3 h**.
  - The wait amortises only when observation tests run concurrently — hence
    `--test-threads >= 16` on Preview (we have ~13 observation tests + headroom).
  - **Preprod / Mainnet** (k = 2160, ~12 h) is out of scope — exceeds GitHub Actions 6 h ceiling.

---

## 3. How to configure the repo and run tests

### 3.1 One-time setup — local-env + e2e

What you need to run the e2e suite against a local-env stack from your dev box:

```bash
# 1. Rust toolchain (pinned in rust-toolchain.toml — direnv will pick it up)
source .envrc
rustup show

# 2. Compile-only sanity check that the e2e target builds (it isn't compiled
#    by plain `cargo check --tests` because [[test]] test = false)
cargo test --test e2e_tests --no-run

# 3. Local-env tooling
brew install just gh           # task runner + GitHub CLI
# Docker Desktop or equivalent — local-env is a docker-compose stack
cd local-environment
npm install                     # one-time install (the npm scripts run ts-node from node_modules)
```

### 3.2 Features that select the target network for the e2e suite

The `tests/e2e` crate has mutually-exclusive features that select where the
suite talks to. **Always** pair `--features X` with `--no-default-features`.

| Feature   | Target node                         | When to use it                                                  |
|-----------|-------------------------------------|-----------------------------------------------------------------|
| `local`   | `ws://127.0.0.1:9933`               | `npm run run:local-env` stack (default local-env port mapping)  |
| `qanet`   | `wss://rpc.qanet.midnight.network`  | qanet (Cardano Preview-backed)                                  |

> Two more features exist (`local-dev`, `local-ci`) for hand-run nodes and
> Earthly-nested Docker respectively — you almost never need them; ignore unless
> you're debugging the CI wiring.

> **`indexer` (opt-in modifier).** Pair with `local` or `qanet`
> (e.g. `--features local,indexer`) to run the `c2m_bridge::*` tests' GraphQL
> assertions against a running indexer-api alongside the node-side checks. Off
> by default; endpoint defaults to `http://127.0.0.1:8088/api/v3/graphql`
> (override via `INDEXER_GRAPHQL_URL`). See `tests/e2e/README.md` for details.

### 3.3 Run the e2e suite — local stack (dev box workflow)

```bash
# A) Start a local-env Cardano + Midnight stack (deps already installed in 3.1)
cd local-environment
npm run run:local-env          # or run:local-env-with-indexer

# B) From repo root, run e2e against it (alias is `cargo test-e2e-local`)
cargo test --test e2e_tests --no-default-features --features local \
  -- --test-threads 6 --no-capture

# Filter by group
cargo test-e2e-local cnight::                  # all cNIGHT
cargo test-e2e-local cnight::observation::     # observation only
cargo test-e2e-local governance::              # all governance
cargo test-e2e-local cnight::alice             # one test (substring match)
```

> The `[[test]]` entry has `test = false`, so `cargo check --tests` does NOT compile e2e.
> To get real compile errors, use `cargo test --test e2e_tests --no-run`.

### 3.4 Run against qanet (Cardano Preview)

```bash
# Requires:
#  - --release (~50× faster replay than dev profile)
#  - TOOLKIT_CACHE_DB_URL pointing at the shared Postgres cache
#  - --test-threads >= 16 for the stability barrier to amortise
TOOLKIT_CACHE_DB_URL="postgresql://..." \
cargo test --release --test e2e_tests --no-default-features --features qanet \
  -- cnight::observation:: --no-capture --test-threads 16
```

> Cardano Preview's stability barrier is ~3 h. Plan accordingly. The nightly workflow
> on GitHub Actions has a 6 h `timeout-minutes` ceiling.

### 3.5 Run against devnet / testnet-02 / preview / preprod (forked, locally)

We **don't run the Rust e2e suite** automatically against these. We do support
*forking* them locally via `local-environment/` for upgrade rehearsals:

- `npm run run:<network>` — fork from a snapshot with mocked authorities.
- `npm run image-upgrade:<network>` — image-only upgrade rehearsal.
- `npm run governance-runtime-upgrade:<network>` — governance runtime upgrade rehearsal.
- `npm run full-upgrade:<network>` — image + governance, sequentially.

For the exact invocations — required env vars (`NODE_IMAGE`, `NEW_NODE_IMAGE`)
and CLI options (`--wasm`, `--council-uris`, `--technical-uris`,
`--executor-uri`, `--from-snapshot`) — see
[`local-environment/README.md`](../../local-environment/README.md).

Networks other than `dev` require AWS access to rebuild genesis — ping the node team.

### 3.6 Rehearsing a Cardano hard fork against local-env

When Cardano announces a new era (PV10 → PV11 / Dijkstra, and any future ones) we
exercise the era transition end-to-end against the local-env stack — what we're
really verifying is that the Midnight Cardano follower keeps observing cleanly
across the fork.

- Script: `local-environment/src/networks/local-env/hardfork-pv11.sh`
- What it does, in order:
  1. Registers a governance stake address, **Constitutional Committee (CC) hot
     credentials** — the hot signing key a CC member uses to cast votes, paired
     with an offline cold key that authorises it — and a DRep.
  2. Delegates stake to the DRep and an SPO.
  3. Submits the hard-fork governance action.
  4. Votes yes from CC + SPO + DRep.
  5. Waits ~5 epochs for ratification + enactment.
- Prerequisite: a `cardano-node` image that supports the target era, with
  `ExperimentalHardForksEnabled: true` in its config.
- Verify after enactment:
  ```bash
  docker exec cardano-node-1 cardano-cli latest query protocol-parameters \
    --testnet-magic 42 | jq '.protocolVersion'
  ```
- **Reusable pattern for future hard forks** — clone `hardfork-pv11.sh`, update
  the era name and target protocol version, and re-run.

### 3.7 The script-based smoke / toolkit e2e family

These live under `scripts/tests/`, are driven via `just` targets, and are
self-contained docker-compose flows — great for SDETs because they need only
`docker` + `just` + the published images. **Not all of them are wired into
CI today** — some are temporarily disabled, others are manual / reusable-action
only (annotated below):

```bash
just toolkit-e2e <NODE_IMG> <TOOLKIT_IMG>
just toolkit-update-ledger-parameters-e2e <NODE_IMG> <TOOLKIT_IMG>
just toolkit-maintenance-e2e <NODE_IMG> <TOOLKIT_IMG>      # CI-disabled (LEDGER9-TOOLKIT-JS)
just toolkit-contracts-e2e <NODE_IMG> <TOOLKIT_IMG>        # CI-disabled (LEDGER9-TOOLKIT-JS)
just toolkit-mint-e2e <NODE_IMG> <TOOLKIT_IMG>             # CI-disabled (LEDGER9-TOOLKIT-JS)
just toolkit-tokens-minter-e2e [<NODE_IMG> <TOOLKIT_IMG>]  # CI-disabled (LEDGER9-TOOLKIT-JS)
just startup-dev-e2e <NODE_IMG>
just startup-qanet-e2e <NODE_IMG>                          # not scheduled by CI; manual / reusable-action only
just genesis-wallets-undeployed-e2e <NODE_IMG> <TOOLKIT_IMG>
just genesis-wallets-devnet-e2e <NODE_IMG> <TOOLKIT_IMG>   # not scheduled by CI; manual / reusable-action only
just indexer-api-e2e                                       # not scheduled by CI; manual only
```

### 3.8 CI surface — when each test runs

| Trigger                            | Workflow                                         | Tests it runs                                          |
|------------------------------------|--------------------------------------------------|--------------------------------------------------------|
| PR / merge queue                   | `continuous-integration.yml`                     | Build, pallet fixtures, toolkit, chainspec validation, |
|                                    |                                                  | local-environment-tests (Earthly `+local-env-ci`):     |
|                                    |                                                  | stack bring-up → verify-finality → e2e suite           |
|                                    |                                                  | (`--features local`) → toolkit-multi-dest              |
| PR / merge queue / push to `main`  | `continuous-integration-test.yml`                | Earthly `+test` target                                 |
| PR / merge queue / push to `main`  | `continuous-integration-checks.yml`              | Static checks, formatting, lints                       |
| PR                                 | `changes_check.yml`                              | Change-file presence (unless `skip-changes-check-*`)   |
| Nightly (00:00 UTC) + manual       | `nightly-run-cnight-e2e-qanet.yml`               | cNIGHT observation suite vs **qanet** (`--features     |
|                                    |                                                  | qanet`, `--test-threads 16`, `--release`)              |
| Nightly + manual                   | `nightly-build-check.yml`                        | Plain build / sanity                                   |
| Comment `/bot …`                   | `cargo-fmt-bot.yml`,                             | Triggered rebuilds for metadata / chainspec / fmt      |
|                                    | `rebuild-metadata-bot.yml`,                      |                                                        |
|                                    | `rebuild-chainspec-bot.yml`                      |                                                        |
| Manual / release                   | `release-image.yml`, `srtool-build.yml`,         | Publish images, reproducible wasm, sbom & security     |
|                                    | `sbom-scan-image.yml`, `security-audit-scan.yml` | scans                                                  |

### 3.9 Release process — testing evidence

For every release candidate we produce a **test-evidence** doc:

- Path: `docs/releases/<version>/test-evidence/test-evidence-<version>-rc.N.md`
- Example: [`docs/releases/1.0.0/test-evidence/test-evidence-1.0.0-rc.8.md`](https://github.com/midnightntwrk/midnight-node/blob/main/docs/releases/1.0.0/test-evidence/test-evidence-1.0.0-rc.8.md)

Workflow:

1. **Take the release notes** (e.g. https://github.com/midnightntwrk/midnight-node/releases/tag/node-1.0.0).
2. **Static set** — copy in the smoke/regression checklist (SMK-01..SMK-11).
3. **Release-specific cases** — write TC-N entries derived from each headline
   change in the release notes (e.g. `transaction_version` bump, new storage migration,
   networkId boot check, ledger API rev, …).
4. Mark rows as ☐ / ✅ / ❌ / ⏭ as you execute them.
5. Open as PR for sign-off; commit once the rc is closed.

The rc.8 doc is the canonical template — clone it.

---

## 4. How to implement new tests

> Operational reference for the e2e suite — pre-deploy gate, stability barrier,
> toolkit cache, `indexer` feature, layout, logging — lives in
> [`tests/e2e/README.md`](../../tests/e2e/README.md).

### 4.1 Decide where the test belongs

```
Pure logic?                      ─►  Inline `#[test]` in the relevant crate
Pallet behaviour?                ─►  pallets/<name>/src/tests.rs (mock runtime)
RPC / extrinsic boundary?        ─►  tests/e2e/tests/<topic>.rs
Cross-stack (Cardano <-> Mid.)?  ─►  tests/e2e/tests/cnight/observation.rs (uses
                                     stability barrier + toolkit-cache warmup)
Pure shell / docker scenario?    ─►  scripts/tests/*.sh + add a `just` target
                                     + add a job in continuous-integration.yml
Release-specific?                ─►  docs/releases/<ver>/test-evidence/...md (TC-N)
                                     + back it with an automated case if it's a
                                     check we'll want forever
```

### 4.2 Anatomy of an e2e test (Rust)

Each test:

1. **Registers its seed** with `register_test_seed(seed)` immediately after generating
   a random `WalletSeed`. This is what enables the toolkit-cache warmup to batch.
2. **Acquires a `PreDeployGuard`** for the body if it asserts pre-deploy behaviour.
3. **Drives chain state.** Either directly via subxt against Midnight RPC (for
   contract / governance / RPC-abuse style tests), or via `whisky` / `ogmios-client`
   against Cardano (for tests that depend on follower-inserted state, e.g. cNIGHT
   observation).
4. **Waits on the relevant assertion target** using the appropriate await helper.
   For cNIGHT observation, that's `await_cnight_observations(tx_ids)` — it subscribes
   to Midnight blocks until every requested Cardano tx_id has been observed in a
   `process_tokens` extrinsic, implicitly handling the stability barrier. Other
   surfaces have their own await helpers under `tests/e2e/src/api/`.
5. **Asserts** against the relevant surface — RPC state, toolkit output, etc.

Reference reads:
- `tests/e2e/tests/cnight/observation.rs` (`cnight_produces_dust`) — canonical
  Cardano-driven example.
- `tests/e2e/tests/c2m_bridge.rs` (`bridge_transfer_cnight_to_midnight_address`)
  — canonical bridge + opt-in `indexer`-feature example.
- `tests/e2e/tests/lib.rs` — gates and global statics.

### 4.3 Add a new test to the suite — checklist

- [ ] Place the `#[tokio::test]` fn in the right module under `tests/e2e/tests/`.
- [ ] If pre-deploy assertion: take a `PreDeployGuard` for the body.
- [ ] If it needs DUST: call `register_test_seed(seed)` right after generation.
- [ ] Use the existing API helpers in `tests/e2e/src/api/` (`midnight.rs`,
      `cardano.rs`, `indexer.rs`) rather than building raw subxt calls — keeps
      logs and retries consistent.
- [ ] Keep it feature-agnostic where possible. If something only makes sense locally,
      gate with `#[cfg(any(feature = "local", feature = "local-dev", feature = "local-ci"))]`.
- [ ] Run with `--features local` against `npm run run:local-env` to validate locally.
- [ ] Open a PR; CI will run it via `+local-env-ci`. The qanet nightly currently
      filters to `cnight::observation::` only — tests outside that module need
      `nightly-run-cnight-e2e-qanet.yml`'s filter expanded to get qanet coverage.

### 4.4 Adding a new shell-level e2e

1. Create `scripts/tests/my-new-e2e.sh` — model on `scripts/tests/toolkit-e2e.sh`.
2. Source `scripts/tests/lib/wait-for-node.sh` for the health-check helper.
3. Always use `set -euxo pipefail`.
4. Always trap-cleanup containers / networks / tempdirs.
5. Add a `just my-new-e2e NODE_IMAGE TOOLKIT_IMAGE` target.
6. Wire it into `continuous-integration.yml` via the `reusable-e2e-tests` action.

### 4.5 How to analyze & debug failed test results

- **Verbose logs:** always pass `-- --no-capture`. Default tracing filter is in
  `tests/e2e/src/logger.rs`; override per-run with `E2E_LOG=...`:
  ```fish
  E2E_LOG=info cargo test --features qanet ...
  E2E_LOG=debug,subxt=warn cargo test ...
  ```
- **`midnight_ledger::semantics=warn` is intentional** — replay would otherwise drown
  the logs in privileged-tx audit lines. Bump to `info` if you need that audit trail.
- **`RUST_BACKTRACE=1`** is set in the nightly workflow — set it locally too for failures.
- **Progress logs from the stability wait:** look for lines like
  `await_cnight_observations: still waiting; midnight #..., cardano: tip=... target=... (... blocks to stability), N/M observed`.
  These tell you *what* the test is blocked on without you having to instrument.
- **Toolkit cache:** `warmup: completed for K seed(s)` with K < your test count means
  you under-threaded — raise `--test-threads`.
- **Nightly failures Slack-notify** to the webhook in `SLACK_WEBHOOK_E2E_NODE`. The
  GitHub Actions run URL is in the message — open it first, not the repo.
- **Local-env stack debugging:** `local-environment/check-health.sh -u http://localhost:9933 -b 50 -t 360`,
  plus `docker logs midnight-setup` / `docker logs contract-compiler` for bring-up failures.

---

## 5. Tips and tricks

### 5.1 Test data

- **Local-env runtime values:** `local-environment/src/networks/local-env/runtime-values/`
  — contracts-info.json, plutus-local.json. e2e/`config.rs` reads these.
- **Cardano payment material for tests:** committed test signing keys in
  `tests/e2e/src/config.rs` (`funded_address*`) — Preview tADA faucet.
- **cNIGHT scripts on Cardano:** `scripts/cnight-generates-dust/` — register, deregister,
  mkCollateral, mkWallets, mkHashes, receive_cnight, rotate_tokens, mainnet bundle.
- **Toolkit fetch cache:**
  - local-env: in-memory.
  - qanet: shared **Postgres** at the URL in `TOOLKIT_CACHE_DB_URL` (NLB-allowlisted).
- **Warm wallet snapshots:** `tests/e2e/toolkit_cache/ledger_cache_db/` — written by
  `dust_balance::execute_many` during warmup.

### 5.2 Configuration cheat sheet

| Variable                  | Purpose                                                                 |
|---------------------------|-------------------------------------------------------------------------|
| `CFG_PRESET`              | Network preset name — selects chainspec preset (see `res/cfg/` for the full set) |
| `E2E_LOG`                 | Override tracing filter for the e2e binary                              |
| `E2E_FAUCET_WORKERS`      | Concurrent faucet workers (set to 16 in nightly to match test threads)  |
| `E2E_FEATURE`             | Feature to compile e2e against (used by the nightly workflow)           |
| `E2E_SKIP_DEPLOY_GATE`    | Bypass pre-deploy gate for subset runs that only pick deploy tests      |
| `RUNTIME_VALUES_DIR`      | Override path to `runtime-values` (contracts-info.json, plutus-local)   |
| `TOOLKIT_CACHE_DB_URL`    | Postgres URL for the toolkit fetch cache (required on qanet)            |
| `RUST_BACKTRACE`          | `1` or `full` — always set on CI                                        |
| `BOOTNODES`               | Used by `sync-with-*.sh` to attach to a remote network's boot nodes     |

### 5.3 Useful repo scripts (not always obvious from `ls`)

| Script                                          | What it does                                                        |
|-------------------------------------------------|---------------------------------------------------------------------|
| `scripts/genesis/genesis-construction.sh`       | Interactive wizard to build a genesis state                         |
| `scripts/genesis/genesis-verification.sh`       | Interactive wizard to verify a genesis state                        |
| `scripts/generate-genesis-seeds.py`             | Generate validator/genesis seeds                                    |
| `scripts/generate-keys.py`                      | Generate session/account keys                                       |
| `scripts/sync-test/build-snapshot.sh`           | Build a minimal cardano-db-sync snapshot for offline sync tests     |
| `scripts/sync-test/run-sync.sh`                 | Spin up postgres with that snapshot + run the node to block N       |
| `scripts/sync.sh`                               | Generic sync driver                                                 |
| `scripts/test-toolkit.sh`                       | Smoke-test the toolkit binary                                       |
| `scripts/analyse_runtime.sh`                    | Inspect runtime metadata (size, modules)                            |
| `scripts/benchmark/generate-reference-hardware.sh` | Generate Substrate weights against reference hardware            |
| `scripts/verify-binary.sh`, `verify-image.sh`   | Verify a released binary / Docker image                             |
| `scripts/free-disk-space.sh`                    | Clean ephemeral GitHub runners before a big build                   |
| `scripts/genesis_wallets_test.sh`               | Validate genesis wallets behaviour                                  |
| `scripts/generate-utxo-ordering-overrides.sh`   | Generate the `utxo-ordering.sql` override snippets                  |
| `sync-with-qanet.sh`, `sync-with-testnet-02.sh` | Bring up a local node attached to qanet / testnet-02 boot nodes     |
| `local-environment/src/networks/local-env/hardfork-pv11.sh` | Drive a Cardano PV10 → PV11 (Dijkstra) hard fork on local-env (CC + SPO + DRep votes, ~5 epochs); reusable template for future hard forks |
| `partnerchains-dev.sh`                          | Dev helper for the partner-chains submodule                         |

### 5.4 PR bots (saves a local rebuild)

Comment on the PR to trigger:

- `/bot rebuild-metadata` — pallet storage / extrinsic / runtime API changes.
- `/bot rebuild-chainspec <network1> <network2>` — chainspec changes per network.
- `/bot cargo-fmt` — auto-apply formatting.

### 5.5 Known issues / gotchas

- **`cargo check --tests` does NOT compile e2e** — see `tests/e2e/Cargo.toml`,
  `[[test]] test = false`. Use `cargo test --test e2e_tests --no-run` instead.
- **Substring filter footgun** — `cargo test dust_balance_smoke` matches BOTH
  `dust_balance_smoke` and `dust_balance_smoke_many`. Use the exact name or
  `--test-threads 1` to serialise (the ledger replay carries process-global state).
- **Dev profile vs release on qanet** — debug build is ~50× slower than release on
  qanet's ~1 M-block chain. Always `--release` on qanet.
- **Substring `--features local` vs `local-dev`** — `--features local` expects port
  9933 (local-env), `local-dev` expects 9944 (handrolled `./target/release/midnight-node`).
- **Cardano Preprod / Mainnet** (k = 2160, ~12 h wait) is out of scope for CI.
  No compile-time gate prevents it — just don't.
- **Self-hosted runner ownership** — the nightly job runs in a container as root;
  it `chown`'s the workspace back at the end so subsequent host-mode jobs aren't
  poisoned. If you add new container-based jobs, copy that pattern.
- **WASM build failure on newer clang** (`error: call to undeclared library function 'memmove'`)
  — demote to non-fatal via `~/.cargo/config.toml` (see CLAUDE.md).
- **Don't force-push** — repo convention. Add a `chore: fix …` commit instead.
- **Commits must be signed**, and the message must follow Conventional Commits.
- **AI-assisted PRs:** add `Assisted-by: AGENT_NAME:MODEL_VERSION` (not `Co-authored-by`),
  and apply the `ai-assisted` label.

### 5.6 Links — repos, tools, docs

**Repos**
- Node: https://github.com/midnightntwrk/midnight-node
- Partner Chains (vendored — not a submodule): `partner-chains/`
- Submodules (see `.gitmodules`):
  - Indexer — https://github.com/midnightntwrk/midnight-indexer (mounted at `indexer/`)
  - Reserve contracts — https://github.com/midnightntwrk/midnight-reserve-contracts (mounted at `midnight-reserve-contracts/`)
  - Compact compiler — https://github.com/LFDT-Minokawa/compact (mounted at `compact/`; pinned in `COMPACTC_VERSION`)

**Videos** (Google Drive — internal)

*Architecture / overview*
- [Node architecture](https://drive.google.com/file/d/1agsL-enu54e3nIkH4LeQi0RYB83CvlPB/view)
- [Oscar's Node Overview](https://drive.google.com/file/d/1fkDFCY4QbFX3NEGXkZ2P69GCeVxQvAfj/view)

*Local-env & ephemeral environments*
- [local-env overview (+ ephemeral envs plans)](https://drive.google.com/file/d/1jl26RBmrWeE9geVS7l59vve7mfFFr4Bv/view)
- [Ephemeral env demo (a.k.a. midnight-up)](https://drive.google.com/file/d/1fl4lH5OJ8Iz_2Re0lvV1eZjTPiy5XENt/view)

*cNIGHT → DUST*
- [cNIGHT generates DUST](https://drive.google.com/file/d/11FK-EYlw3kDxEMemG4DZnRsmLTrDEIJB/view)
- [cNgD vs mNgD — how to verify Dust balance](https://drive.google.com/file/d/1cNvCz4oXPhEzekcVbNAIRNPPnWuH_HkI/view)
- [Dust Management CLI — init & register](https://drive.google.com/file/d/1JCCF5FSMU6d5yOWmWonbXQEP1Zl3oMFd/view)

*Governance & upgrades*
- [Polkadot UI + sudo + runtime upgrade](https://drive.google.com/file/d/1_5mcbAmWN87b9MXgPPaL_KdpzzvFnu4i/view)
- [How to change DParam?](https://drive.google.com/file/d/1dUajZDXTsZxf55wiv7GhDzQiv8OYfjoU/view)

*Testing / AI*
- [AI usage in e2e tests (based on dust e2e on qanet)](https://drive.google.com/file/d/1N91eMjzblOMAnx2GAjvb5SADyAjVn_lr/view)

**Internal docs**
- `docs/c-to-m-bridge.md` — Cardano → Midnight bridge protocol overview
- `docs/chain_specs.md` — chainspec layout
- `docs/configuration-guide.md` — CLI / env / config
- `docs/development-workflow.md` — dev setup
- `docs/fork-testing.md` — fork rehearsal docs (pairs with `local-environment/`)
- `docs/genesis/{construction,verification}.md` — genesis tooling guide
- `docs/tests/test-plan-*.md` — historical test plans we've shipped
- `docs/releases/<ver>/test-evidence/*.md` — release evidence template

**External / tools**
- Substrate / Polkadot SDK docs — https://docs.substrate.io
- subxt — https://docs.rs/subxt
- Whisky (Cardano tx builder) — https://github.com/sidan-lab/whisky
- Ogmios — https://ogmios.dev
- Cardano db-sync — https://github.com/IntersectMBO/cardano-db-sync
- Earthly — https://docs.earthly.dev
- just — https://github.com/casey/just
- gh CLI — https://cli.github.com

**Networks / endpoints**
- qanet RPC: `wss://rpc.qanet.midnight.network`
- Ogmios devnet: `wss://ogmios.devnet.midnight.network`
- Slack channel: `SLACK_WEBHOOK_E2E_NODE` (nightly results land here)
- Linear / Jira tickets: `PM-*` (see e.g. test plan in `docs/tests/test-plan-d-parameter-pallet-integration.md`)

---

## Appendix A — One-page "where do I look?" map

| If you want to…                                       | Go to                                                              |
|-------------------------------------------------------|--------------------------------------------------------------------|
| Add an automated e2e test                             | `tests/e2e/tests/<topic>.rs`                                       |
| Add a shell-level e2e scenario                        | `scripts/tests/*.sh` + `justfile` + `continuous-integration.yml`   |
| Configure / change the e2e suite at runtime           | `tests/e2e/src/{config.rs,logger.rs,faucet.rs}`                    |
| Tweak the nightly cNIGHT run                          | `.github/workflows/nightly-run-cnight-e2e-qanet.yml`               |
| Tweak the per-PR local-env run                        | `.github/actions/local-environment-tests/action.yml` + `Earthfile` |
| Rebuild metadata / chainspecs                         | `earthly -P +rebuild-metadata` or `/bot rebuild-…`                 |
| Fork a real network for upgrade rehearsal             | `local-environment/` (`npm run run:<net> -- --from-snapshot ...`)  |
| Sign off a release                                    | `docs/releases/<ver>/test-evidence/test-evidence-<ver>-rc.N.md`    |
| Reproduce a Cardano sync issue locally                | `scripts/sync-test/{build-snapshot,run-sync}.sh`                   |
| Verify a released binary or image                     | `scripts/verify-binary.sh`, `scripts/verify-image.sh`              |


---

