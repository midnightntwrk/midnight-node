# Midnight Toolkit — Product Requirements Document

> Retroactive PRD describing the Midnight Toolkit as it exists today, plus
> a short forward-looking section on planned work.
>
> **Audience:** broader Midnight org — primarily product and architecture.
>
> **Last updated:** 2026-04-28.
> 
> **Author(s):** Oscar Bailey

---

## Contents

1. [Purpose & summary](#1-purpose--summary)
2. [Background & context](#2-background--context)
3. [Users & personas](#3-users--personas)
4. [Use cases / user journeys](#4-use-cases--user-journeys)
5. [Goals & non-goals](#5-goals--non-goals)
6. [Functional requirements](#6-functional-requirements)
7. [Non-functional requirements](#7-non-functional-requirements)
8. [Constraints & dependencies](#8-constraints--dependencies)
9. [Success metrics](#9-success-metrics)
10. [Risks & open questions](#10-risks--open-questions)
11. [Future work](#11-future-work)

---

## 1. Purpose & summary

The Midnight Toolkit is a CLI for interacting with the Midnight network. It
exercises a large proportion of the functionality provided by the Node,
Ledger, and Compact, with its primary capabilities being to build, send, and
inspect transactions, and to perform governance operations.

The toolkit's core value is that it lets the team test core Midnight
functionality without depending on updates to downstream components. It
provides a low-level interface to capabilities that, in production, are
normally surfaced through higher-level systems: transaction building
(otherwise via the wallet), contract interactions (otherwise via
midnight-js), and wallet state construction (otherwise via the indexer).
Because it sits closer to the protocol, it is cheaper to maintain than those
downstream stacks and can move in lockstep with the Node and Ledger.

The toolkit presents a unified interface for transaction building,
wallet-state querying, and governance actions. Its other sub-commands exist
to support those primary use cases. It has also been used extensively by the
performance team to load-test the network.

---

## 2. Background & context

**Origins.** The toolkit began as a "transaction-generator" — a tool for
testing the node without needing to stand up a wallet and indexer. Initially
it was a collection of disjointed transaction-building commands. In April
2025 transaction generation was unified behind a single abstraction and the
tool was [renamed to the toolkit](https://github.com/midnightntwrk/midnight-node-old/pull/757).

**The "before" picture.** Three concrete pain points motivated and continue
to justify the toolkit:

1. *Generating a simple shielded transaction on a test network* previously
   required running a wallet, an indexer, and a node together. This is
   particularly painful around ledger version changes: a new ledger version
   requires a new node, which requires a new indexer, which requires a new
   wallet — a long serial chain.
2. *QA-ing a new CompactC version* previously required updating midnight-js
   and building a DApp. Once contract-calling was integrated into the
   toolkit, the loop became `node + toolkit + compactc + a .compact file` —
   dramatically faster, particularly for testing new language features.
3. *Governance operations* — including ledger parameter updates and runtime
   upgrades — were previously done with ad-hoc scripts driving polkadot-js.
   They can now be performed end-to-end with the toolkit.

**Where the toolkit sits relative to other Midnight components.** The
toolkit deliberately re-implements parts of what downstream components do:

- *Fetch, and ledger-state verification* are re-implemented from the indexer.
- *Transaction building* is re-implemented from the wallet / midnight-js.
- *Contract-call construction* is shared with midnight-js via a common
  wrapped library
  ([`compact.js`](https://github.com/midnightntwrk/midnight-sdk/tree/main/compact-js)),
  which wraps the compact runtime with higher-level primitives for
  building transactions that include contract calls.

This duplication is intentional. It means the toolkit is independent of
those components, and it gives the team a powerful diagnostic lever: for
example, if the toolkit can sync and send transactions but the indexer and
wallet cannot, the bug is in indexer/wallet, not in the node or ledger.

```
   Production end-user stack                Toolkit
   ─────────────────────────                ───────

   ┌──────────────┐                    ┌──────────────┐
   │ DApp / user  │                    │   CLI user   │
   └──────┬───────┘                    └──────┬───────┘
          │                                   │
          ▼                                   │
   ┌──────────────┐                           │
   │ midnight-js  │── compact.js (shared) ────┤
   └──────┬───────┘                           │
          │                                   │
          ▼                                   │
   ┌──────────────┐  ┌──────────────┐         │
   │    Wallet    │  │   Indexer    │         │ (re-implements
   └──────┬───────┘  └──────┬───────┘         │  tx-building +
          │                 │                 │  fetch / state
          ▼                 ▼                 ▼  verification)
   ┌─────────────────────────────────────────────────────┐
   │   Midnight Node (JSON-RPC) — Ledger, Runtime        │
   └──────────────────────────┬──────────────────────────┘
                              │
                              ▼  (bridge: cNIGHT / DUST —
   ┌─────────────────────────────────────────────────────┐
   │   Cardano                  bridge_transfer is an    │
   │                            incomplete new toolkit   │
   │                            extension)               │
   └─────────────────────────────────────────────────────┘
```

---

## 3. Users & personas

The toolkit is an **internal-only** tool — it is not a Shielded Technologies
product. Its users are five cohorts within the Midnight org.

### Node / protocol engineers

Day-to-day development against the node, ledger, and runtime. Use the
toolkit to build and inspect single transactions and check wallet/chain
state without standing up a full wallet + indexer stack.

*Centre of gravity:* `single-tx`, `show-wallet`, `show-address`,
`show-block`.

### Performance / load-testing engineers

Drive throughput and stress tests against networks. Pre-generate large
batches of transactions and push them at controlled rates.

*Centre of gravity:* `batch-single-tx`, `send-tx --rate <RATE>`.

### QA

Covers two overlapping concerns:

- **Compact / language** — validating new CompactC features and contract
  behaviour end-to-end. A new `.compact` contract file can be exercised
  against a real node without touching midnight-js or building a DApp.
- **Ledger** — validating new ledger versions and transaction-level
  semantics: that valid transactions apply, invalid ones reject, ZK
  proofs verify, and state transitions match expectations across ledger
  version transitions. This is the cohort whose sign-off gates a ledger
  upgrade (see Section 9.1).

*Centre of gravity:* `generate-intent` (Compact); `generate-txs single-tx`,
`generate-txs batches`, `show-block`, `show-wallet`, `show-transaction`
(Ledger).

### SRE — node operations & governance

Execute governance actions on networks where a single actor controls the
relevant seeds — primarily ledger parameter updates and runtime upgrades.

*Centre of gravity:* `runtime-upgrade`, `update-ledger-parameters`.

> **Important constraint.** The toolkit's governance commands require all
> signing seeds to be passed via the CLI. This means the toolkit is only
> usable for governance on networks where one actor controls every seed
> (e.g. dev / qanet). It is **not** usable on production-style networks
> where seeds are split across multiple holders. Adding a single-key
> governance mode (where each holder partially signs and the toolkit
> assembles or hands off the partial) is noted in *Future work*. See
> also *Constraints & dependencies* (Section 8).

### Indexer team

Use the toolkit to generate test data for the indexer — producing
representative transactions and chain history that the indexer can be
exercised against, without depending on a wallet or DApp to source the
traffic.

*Centre of gravity:* `generate-txs` (single-tx, `batches`,
`batch-single-tx`), with file-based destinations for replay-style
fixtures.

### Explicit non-users

- **End users of Midnight DApps.** They use the wallet and DApp UI, not
  the toolkit.
- **DApp developers.** They target midnight-js. The toolkit interface is
  lower-level than DApp developers likely need.
- **Node operators.** Node operators currently use the Polkadot Apps UI
  for governance actions:
  <https://polkadot.js.org/apps/#/explorer>.
- **External partners** and the wallet / midnight-js teams themselves.
  Not currently positioned as toolkit users. (The indexer team *is* a
  user — see above.)

---

## 4. Use cases / user journeys

Eight representative journeys. Examples are abbreviated; the README and the
end-to-end test scripts under [`scripts/tests/`](../../scripts/tests/) have
the full forms.

### 4.1 Smoke-test a code change against a local node *(node engineer)*

After a node, ledger, or runtime change, generate a single shielded +
unshielded transaction against a locally-running node and confirm it lands
and applies cleanly.

```console
$ midnight-node-toolkit generate-txs single-tx \
    --shielded-amount 100 --unshielded-amount 5 \
    --source-seed 0000…0001 \
    --destination-address mn_shield-addr_undeployed1…
```

### 4.2 Validate a new CompactC feature *(language QA)*

Compile a `.compact` contract, deploy it, exercise a circuit that mutates
balances, and verify the result by inspecting the calling wallet — without
touching midnight-js or building a DApp.

The closest real example is
[`scripts/tests/toolkit-mint-e2e.sh`](../../scripts/tests/toolkit-mint-e2e.sh):
deploy a mint contract, mint tokens to a shielded address, then confirm the
new shielded coin shows up in the wallet's view.

```console
# 1. Generate and send the deploy intent
$ midnight-node-toolkit generate-intent deploy \
    -c contract/mint.config.ts \
    --output-intent out/deploy.intent …
$ midnight-node-toolkit send-intent --intent-file out/deploy.intent …

# 2. Generate and send a circuit-call intent (e.g. `mint`)
$ midnight-node-toolkit generate-intent circuit \
    -c contract/mint.config.ts \
    --contract-address $contract_addr \
    --output-intent out/mint.intent \
    mint <nonce> <domain_sep> 1000
$ midnight-node-toolkit send-intent --intent-file out/mint.intent …

# 3. Verify by checking the wallet shows the new coin
$ midnight-node-toolkit show-wallet --seed 0000…0001 \
    | grep "$(midnight-node-toolkit show-token-type \
                --contract-address $contract_addr --domain-sep <…> --unshielded)"
```

### 4.3 Load-test the network *(performance engineer)*

Pre-generate a large batch of single-transfer transactions to a file, then
push them at a controlled rate.

```console
$ midnight-node-toolkit generate-txs --dest-file txs.json \
    batch-single-tx --transfers-file transfers.json
$ midnight-node-toolkit generate-txs -r 50 \
    --src-file txs.json --dest-url ws://localhost:9944 send
```

### 4.4 Pre/post runtime-upgrade transaction smoke test *(node engineer working on a ledger update)*

Confirm that a new ledger version applies cleanly on a running chain by
generating and sending a Midnight transaction *before* the runtime upgrade,
executing the upgrade, then sending another transaction *after*.

```console
$ midnight-node-toolkit generate-txs single-tx …          # pre-upgrade tx
$ midnight-node-toolkit runtime-upgrade \
    --wasm-file midnight_node_runtime.compact.compressed.wasm \
    -c //Alice -c //Bob -t //Dave -t //Eve --signer-key //Alice
$ midnight-node-toolkit generate-txs single-tx …          # post-upgrade tx
```

### 4.5 Build a new genesis state *(genesis / chainspec author — release-time)*

Generate a genesis ledger state for a network preset. Used at release time
and when the genesis seeds, genesis code, or ledger version changes — not a
daily journey.

In practice this is invoked through the `+rebuild-genesis-state` Earthly
target ([`Earthfile`](../../Earthfile) line 197), which wraps the toolkit
command with the right inputs per network. There is one wrapper target per
network (`+rebuild-genesis-state-undeployed`,
`+rebuild-genesis-state-qanet`, `+rebuild-genesis-state-preview`, …).

```console
$ earthly -P +rebuild-genesis-state-<network>
# under the hood:
#   midnight-node-toolkit generate-genesis --network <network> …
```

### 4.6 Execute a runtime upgrade on a non-prod network *(QA / SRE)*

Apply a new runtime to a network where the operator holds all governance
seeds. The toolkit drives the entire Council + Technical Committee approval
flow plus the apply step.

```console
$ midnight-node-toolkit runtime-upgrade \
    --wasm-file /path/to/midnight_node_runtime.compact.compressed.wasm \
    -c <COUNCIL_KEY_1> -c <COUNCIL_KEY_2> \
    -t <TC_KEY_1> -t <TC_KEY_2> \
    --rpc-url ws://localhost:9944 --signer-key //Alice
```

### 4.7 Update ledger parameters on a non-prod network *(QA / SRE)*

Change ledger parameters (e.g. a bridge minimum) via the same
federated-authority flow.

```console
$ midnight-node-toolkit update-ledger-parameters \
    -t //Alice -t //Bob -c //Dave -c //Eve \
    --c-to-m-bridge-min-amount 2000
```

### 4.8 Diagnose a downstream-component bug *(node engineer)*

When the wallet or indexer is misbehaving, confirm whether the node is
serving correct data by syncing wallet state through the toolkit
independently. If the toolkit's wallet view is correct but the wallet /
indexer's is not, the bug is downstream.

```console
$ midnight-node-toolkit show-wallet \
    --src-url ws://localhost:9944 \
    --seed 0000…0001
```

### 4.9 Automated CI integration tests *(journey-of-the-system)*

The toolkit is also a load-bearing piece of CI. A family of
[`scripts/tests/toolkit-*-e2e.sh`](../../scripts/tests/) scripts spin up a
Midnight node in Docker and drive end-to-end scenarios through the toolkit;
the same scripts run locally via `just toolkit-e2e <NODE_IMAGE>
<TOOLKIT_IMAGE>` and as named jobs in
[`.github/workflows/continuous-integration.yml`](../../.github/workflows/continuous-integration.yml).

The most load-bearing scripts:

| Script | What it covers |
|---|---|
| `toolkit-e2e.sh` | Base happy path: shielded + unshielded transfers, address generation, DUST registration / deregistration, built-in contract deploy + call. |
| `toolkit-contracts-e2e.sh` | Built-in contract deploy, call, and state inspection. |
| `toolkit-mint-e2e.sh` | Custom-contract mint flow: deploy a `.compact` mint contract, call its circuit, verify the resulting shielded coin appears in `show-wallet`. |
| `toolkit-maintenance-e2e.sh` | Contract maintenance updates (entrypoint upserts/removals, authority rotation). |
| `toolkit-update-ledger-parameters-e2e.sh` | Federated-authority ledger parameter update, end-to-end. |
| `toolkit-tokens-minter-e2e.sh` | Token-minter contract variants. |

The multi-destination-send no-hang check moved to the Rust e2e suite as
`operational::toolkit_multi_dest_send_does_not_hang` (runs against the local-env,
funded by the cNIGHT bridge), replacing the former `toolkit-multi-dest-e2e.sh`.

Because these scripts exercise the same CLI users invoke by hand, any
regression in a user-facing command also breaks CI — which gives the
toolkit a continuous integration safety net beyond its unit tests.

---

## 5. Goals & non-goals

### Goals

1. **Provide a direct, low-level interface to the Midnight network** —
   closer to the protocol than the wallet, indexer, or midnight-js.
2. **Decouple core-protocol testing from downstream component readiness.**
   The team can build, send, and inspect transactions before the wallet,
   indexer, or midnight-js have caught up to a new node / ledger version.
3. **Move in lockstep with the node and ledger** — bumped on the same
   cadence, sharing types and code where it makes sense.
4. **Cover the four core capability areas** — transaction building,
   wallet-state querying, governance, and contract interactions — under
   one unified interface.
5. **Support load and stress testing** of the network at meaningful
   throughput.
6. **Be a load-bearing CI dependency.** The same CLI humans use should be
   the one CI exercises end-to-end.
7. **Be cheap to extend.** Adding a new transaction builder, governance
   call, or query does not require touching shared infrastructure; the
   `Builder` trait and the documented "Add a new Builder" / "Add a new
   Contract" extension paths in the README are the canonical entry
   points.

### Non-goals

1. **Not a production end-user wallet.** No key management beyond
   seeds-on-the-CLI; not intended for asset custody or daily user use.
2. **Not a DApp development toolkit *today*.** DApp authors target
   midnight-js. The toolkit *could* serve quick prototyping or test-running
   use cases for DApp authors in future, but it is not positioned that way
   right now.
3. **Not a production-grade governance tool.** Governance commands
   require all signing seeds on the CLI host; usable on dev / qanet, not on
   networks where seeds are split across holders.
4. **Not a public API or external product.** Internal-only. The team
   actively avoids breaking CLI changes because of the many downstream
   consumers (the perf team, CI scripts, ops scripts), but the CLI is not
   contractually stable in the way a published external API would be.
5. **Not an observability / monitoring tool.** Inspection commands
   (`show-wallet`, `show-block`, `dust-balance`) exist for human use
   during testing, not for production telemetry.

---

## 6. Functional requirements

The toolkit's capability surface, grouped by area. Each entry lists the
commands that fulfil it.

### 6.1 Transaction generation

Build native Midnight transactions: single transfers, batches, and
specialised flows. Sources, destinations, prover, and rate are configured
independently of the builder.

| Capability | Commands |
|---|---|
| Single transfer (shielded + unshielded) | `generate-txs single-tx` |
| Pre-defined batch of single transfers | `generate-txs batch-single-tx --transfers-file …` |
| Generated batches (zswap / unshielded UTxO) | `generate-txs batches -n <N> -b <B>` |
| Send pre-built transactions | `generate-txs send` |
| DUST address registration / deregistration | `generate-txs register-dust-address`, `generate-txs deregister-dust-address` |
| Sources | chain (`--src-url`), file (`--src-file`, repeatable) |
| Destinations | chain (`--dest-url`, multi), file (`--dest-file`) |
| Prover | local (default), remote proof server |
| Rate control | `-r <TPS>` |
| Dry-run | `--dry-run` for plan-only execution |

### 6.2 Contract calls and contract maintenance

Build, send, and update contract transactions. Custom contracts go through
[`compact.js`](https://github.com/midnightntwrk/midnight-sdk/tree/main/compact-js)
(via [`toolkit-js`](../toolkit-js/), a thin wrapper) — the same contract-call
construction path used by midnight-js.

| Capability | Commands |
|---|---|
| Built-in test contract: deploy / call / maintenance | `generate-txs contract-simple deploy`, `… call`, `… maintenance` |
| Custom contract: deploy intent | `generate-intent deploy` |
| Custom contract: circuit-call intent | `generate-intent circuit` |
| Build a transaction from an intent and submit it | `send-intent` |
| Maintenance: entrypoint upsert / removal, authority + verifier-key rotation | `generate-txs contract-simple maintenance` (built-in), `generate-intent` flows for custom |

### 6.3 Genesis

Construct genesis ledger state for a target network preset. Invoked
day-to-day through the `+rebuild-genesis-state` Earthly target wrappers
(see Section 4.5).

| Capability | Commands |
|---|---|
| Per-network genesis ledger state | `generate-genesis --network <network>` |

### 6.4 Governance

Drive federated-authority flows on networks where all signing seeds are
available locally.

| Capability | Commands |
|---|---|
| Update ledger parameters via Council + TC | `update-ledger-parameters` |
| Apply a runtime upgrade end-to-end | `runtime-upgrade` |
| Generic Root-origin call via governance (primitive used by the above) | `root-call` |

### 6.5 Wallet, chain, and contract inspection

Read-only commands for inspecting addresses, balances, blocks, ledger
state, and contracts.

| Capability | Commands |
|---|---|
| Wallet view (UTXOs, coins, DUST, seeds derived) | `show-wallet` |
| DUST balance with per-output breakdown | `dust-balance` |
| Address derivation from seed | `show-address` |
| Random address generation (for fixtures) | `random-address` |
| Block inspection | `show-block` (`--block-number`, `--src-file`, `--json`) |
| Transaction inspection | `show-transaction` |
| Token type derivation | `show-token-type` |
| Viewing-key extraction | `show-viewing-key` |
| Seed display | `show-seed` |
| Ledger parameters (with optional overrides) | `show-ledger-parameters` |
| Derive contract address from a deploy tx | `contract-address` |
| Fetch on-chain contract state (serialized) | `contract-state` |

### 6.6 Cardano bridge (incomplete extension)

Bridge-related transaction support for the cNIGHT / DUST bridge. This
surface is in active development; do not treat it as stable.

| Capability | Commands |
|---|---|
| Cardano → Midnight bridge transfer | `bridge-transfer` |

### 6.7 Fetching chain data

The toolkit can pull historical and current chain data either from a node
RPC or from local files. Fetched chain data is cached locally to avoid
redundant network round-trips; the cache is shared across commands.

| Capability | Commands / flags |
|---|---|
| Fetch from node RPC | `--src-url ws://…` (used across commands) |
| Fetch from file(s) | `--src-file …` (repeatable) |
| Local cache backends | in-memory (default), `redb:<file>` (single-process, persistent), `postgres://…` (concurrent readers/writers) |
| Cache-only read (skip network) | `--fetch-only-cached` |
| Standalone fetch | `fetch` |

### 6.8 Cross-cutting

| Capability | Notes |
|---|---|
| Version & compatibility reporting | `version` (Node, Ledger, Compactc) |
| JSON output | `--json` on inspection commands |
| Dry-run | `--dry-run` plans without executing |
| Multi-source / multi-destination | repeatable `--src-file`, multiple `--dest-url` |
| Network-id selection | inferred from source; `--network <name>` for derivation commands |
| **Multi-ledger-version support** | The toolkit supports every ledger version currently running on Midnight's live production chains. Per-version logic lives under [`src/commands/fork/`](src/commands/fork/) (e.g. `ledger_7.rs`, `ledger_8.rs`). |

---

## 7. Non-functional requirements

### 7.1 Distribution & install

- Docker image: `midnightntwrk/midnight-node-toolkit:<tag>`. Tags include
  `latest-main` (recommended for everyday use) and version-pinned tags
  (e.g. `0.22.0`) for guaranteed compatibility with a specific node
  version.
- From source: `cargo install --locked --git
  https://github.com/midnightntwrk/midnight-node midnight-node-toolkit`,
  optionally with `--tag node-<version>`.
- Published as part of the node release pipeline.

### 7.2 Version compatibility (explicit commitment)

The `latest-main` toolkit image is committed to remaining **backwards
compatible with previous node and ledger versions that are in use in
production or pre-production**. Practically, this means a single
`latest-main` toolkit can be pointed at a qanet, preview, preprod, or
mainnet node and continue to build, send, and inspect transactions
correctly across ledger version transitions.

Version-pinned toolkit tags (e.g. `0.22.0`) exist as a safety net for
users who want the exact compatibility envelope of a specific node
version.

The `version` command reports the toolkit's bound versions of Node,
Ledger, and Compactc.

### 7.3 Performance

The toolkit must support large-scale load testing. As a calibration point:
in a recent nightly performance run the toolkit pre-generated and sent
**~55k transactions**, maxing out many consecutive blocks. Rate is
controlled per-command via `-r <TPS>`, and pre-generated batches can be
written to a file and replayed via `generate-txs send` to decouple
proof-time from send-time.

### 7.4 Reproducibility / determinism

- Seeded RNGs everywhere generation is randomised (`--rng-seed`,
  `--randomness-seed`).
- Dry-run mode for plan-only execution.
- File-based sources and destinations (`--src-file`, `--dest-file`) make
  fixtures reproducible across CI runs.

### 7.5 Observability

- Human-readable output by default; `--json` for machine-readable
  inspection output.
- `--verbose` mode for deeper debugging (more detailed structured logs).
- Dry-run output describes the plan in detail (sources, destinations,
  builder configuration, prover) before any side-effects.

### 7.6 Operational footprint

- Single static binary, or runs as a self-contained Docker container.
- Configurable fetch cache backends per scale: in-memory (default),
  `redb:<file>` (single-process, persistent), or
  `postgres://…` (multi-process, concurrent readers/writers).
- Cache size grows with the volume of fetched chain history; for
  long-running test loops or load tests this can become non-trivial on
  disk and should be cleaned periodically.

### 7.7 Security model (informational)

- All signing seeds and governance keys are passed via CLI flags or files
  on the host running the toolkit. There is no key store, no agent, no
  HSM integration.
- This is fit-for-purpose for internal test networks where one operator
  controls all seeds, and is a hard limit on the toolkit's use in
  multi-party-governance settings.

### 7.8 CLI stability

The CLI is treated as a soft contract: breaking changes are actively
avoided because of the many downstream consumers (the perf team's
harnesses, CI scripts, ops scripts). The toolkit is internal, so the CLI
is not contractually stable in the way a published external API would
be — but in practice the team behaves as if it were.

---

## 8. Constraints & dependencies

### 8.1 Hard constraints

1. **All governance seeds must be available on the CLI host** for any
   command that drives federated-authority flows (`update-ledger-parameters`,
   `runtime-upgrade`, `root-call`). This locks the toolkit out of
   production multi-party governance, where seeds are split across
   holders.
2. **Minimum supported node version: `0.22.0`** (the current mainnet
   version), which corresponds to **Ledger v7**. The toolkit is committed
   to backward compatibility from this floor up through every node /
   ledger version currently in use across production and pre-production
   networks.
3. **Lockstep with node and ledger.** The toolkit is built from the same
   workspace as the node and shares types directly with the runtime; it
   cannot drift from the protocol it speaks to.
4. **Node JSON-RPC reachability.** Live-chain operations require a
   reachable WebSocket endpoint (`ws://…`).
5. **Custom-contract flows require `compactc` and `compact.js` (via
   `toolkit-js`)** on PATH. The published Docker image bundles both;
   from-source builds need to set them up explicitly (`TOOLKIT_JS_PATH`
   env var, see README).
6. **AWS access** is required for genesis rebuilds on non-dev networks
   (per `AGENTS.md`); the `dev` preset has no such dependency.
7. **Platform.** Linux/x86_64 is the supported and tested target (Docker
   image, perf runs). Other platforms have no explicit support contract.

### 8.2 Dependencies

| Dependency | Role | Notes |
|---|---|---|
| `midnight-ledger` | Ledger types, state transitions, ZK proofs | Source of truth for transaction shape and verification. |
| `polkadot-sdk` (Substrate) | Runtime types, RPC plumbing, governance pallets | Inherited via the node workspace. |
| `partner-chains` SDK | Cardano sidechain framework | Used where the toolkit interacts with partner-chain-specific transaction shapes. |
| [`compact.js`](https://github.com/midnightntwrk/midnight-sdk/tree/main/compact-js) (via [`toolkit-js`](../toolkit-js/)) | Contract-call construction primitives | Shared with `midnight-js`; wraps the compact runtime. `toolkit-js` is a thin CLI wrapper that exposes `compact.js` to the toolkit's `generate-intent` flows. Bundled in the Docker image. |
| Proof server (optional) | Remote proof generation | Released alongside the ledger. The toolkit's internal prover suffices for the majority of use cases; remote prover is an opt-in alternative. |
| AWS (non-`dev` networks) | Genesis seed storage / retrieval | Only relevant when (re)building genesis for `qanet`, `preview`, `preprod`, etc. |

### 8.3 Future-tense dependency

A **Cardano node** will become a real dependency once `bridge_transfer`
matures (cNIGHT / DUST bridge testing). Today this is not a present-day
constraint and is scoped under *Future work*.

---

## 9. Success metrics

> *Framing: this is a retroactive PRD. The signals below describe how the
> team currently judges whether the toolkit is doing its job — they are
> not targets being imposed on the project.*

The toolkit is doing its job when:

1. **Ledger / node upgrades pass the toolkit-driven validation that gates
   them.** Ledger upgrades are *by design* gated on the toolkit, because
   the toolkit is the mechanism by which the team validates a new ledger
   version end-to-end. The signal is that toolkit work is not the long
   pole — toolkit support ships as part of the same release stream as
   the ledger change.
2. **The `toolkit-*-e2e.sh` CI jobs are stable.** Flaky or red runs in
   `continuous-integration.yml` are investigated as protocol issues
   first, not toolkit issues.
3. **The performance team uses the toolkit as their default load-test
   driver.** Calibration point: ~55k transactions in a recent nightly
   run.
4. **All non-prod governance actions go through the toolkit** —
   `update-ledger-parameters` and `runtime-upgrade` are the standard
   path, not bespoke polkadot-js scripts.
5. **Engineers can validate node, ledger, and Compact changes against a
   real chain without waiting for indexer, wallet, or midnight-js
   updates.** This is the original raison d'être and remains the most
   important qualitative signal.
6. **Time-to-test for new CompactC features** is short, measured against
   the pre-toolkit baseline of building a DApp through midnight-js.
7. **The `latest-main` backward-compatibility commitment is honoured** —
   the same toolkit binary continues to work against every production
   and pre-production node version currently in use.

---

## 10. Risks & open questions

### 10.1 Risks

1. **Divergence from downstream components.** The toolkit re-implements
   transaction building (vs wallet / midnight-js) and fetch + state
   verification (vs the indexer). If the toolkit's implementation drifts
   from any of these, its diagnostic value collapses — bugs that the
   toolkit would have caught instead get masked because the toolkit and
   the downstream component agree on the wrong answer.
2. **Fetch + cache scalability on long-running chains.** The current
   fetch path and local cache work well for short-lived test networks
   and small chains. They become unwieldy on long-running chains as the
   amount of historical data the toolkit must process grows. Indexer
   integration (see *Future work*) is the planned mitigation.
3. **Multi-ledger-version maintenance surface.** Per-ledger-version forks
   under `src/commands/fork/` accumulate over time. The risk is small —
   the node itself carries the same surface for the same reason — but
   it is real, and shows up in compile times and in the cognitive cost
   of touching shared code paths.
4. **`compact.js` coupling (via the `toolkit-js` CLI wrapper).**
   Custom-contract flows depend on Node.js plus a separately-built
   `toolkit-js` (a thin CLI wrapper around `compact.js`). Environment
   setup is a frequent paper-cut for new users and an extra moving part
   for the Docker image.
5. **CLI as a soft contract** *(low risk).* Many downstream consumers
   depend on the CLI shape (perf team, CI scripts, ops scripts), and
   there is no formal CLI versioning policy. The team avoids breaking
   changes culturally, but a well-intentioned refactor could break a
   consumer silently. Risk is rated low because the cultural norm has
   held in practice.

### 10.2 Open questions

**Closed design questions** *(included for the record):*

- **Multi-ledger-version pruning** — *not pruned.* Mainnet launched on
  Ledger v7 / node `0.22.0`, so that floor is permanent.
- **Indexer support and the existing fetch path** — *coexist.* The
  indexer integration sits alongside the re-implemented fetch path;
  it does not replace it, so the diagnostic independence of the
  toolkit is preserved.

**Open product question:**

- **Is there a gap for an external DApp-developer CLI, distinct from
  the toolkit?** The toolkit is positioned as internal-only and is too
  low-level for DApp authors (Section 5 non-goal #2; Section 3 explicit
  non-users). If the org decides DApp developers need a CLI of their
  own, the natural shape is a separate, TypeScript-based product that
  depends on the same higher-level stack DApp authors already target —
  wallet, indexer, midnight-js, and `compact.js` — rather than an
  extension of the toolkit. Whether to build it, when, and who owns it
  is unresolved.

Other design and implementation questions live in the issue tracker.

---

## 11. Future work

### 11.1 Indexer support

Tracking issue:
[`midnightntwrk/midnight-node#1186`](https://github.com/midnightntwrk/midnight-node/issues/1186).

Integrate the toolkit with the Midnight indexer so it can read chain
state without fetching and verifying the full history itself. This
mitigates the fetch + cache scalability risk on long-running chains
(see Section 10.1) while preserving the existing direct-fetch path
alongside it — keeping the toolkit's diagnostic independence (see
Section 10.2).

### 11.2 Composable contracts (contract-to-contract calls)

Product issue:
[`shieldedtech/product#32`](https://github.com/shieldedtech/product/issues/32).

The README's implementation-status table lists *Composable Contracts* as
in-progress (⏳). Adding contract-to-contract call support to the
toolkit's intent-generation flow lets the team exercise composable
contracts end-to-end against a real chain — without a DApp, the same
way single-contract testing works today.

### 11.3 Support for Cardano -> Midnight bridge testing

Product issue:
[`shieldedtech/product#34`](https://github.com/shieldedtech/product/issues/34).

Bring the cNIGHT / DUST bridge surface to the same maturity bar as the
rest of the toolkit. This brings a **Cardano node** into scope as a
real present-day dependency (see Section 8.3), and unlocks end-to-end
testing of the Cardano -> Midnight bridge.

---
