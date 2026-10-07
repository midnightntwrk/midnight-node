# Ledger 8 -> 9 hard fork tests

End-to-end tests for the ledger 8 -> 9 hard fork: node `1.0.400` (ledger 8) to node `2.1.0`
(ledger 9), first a binary rollout and then a governance runtime upgrade. The same checks run
against a local-env chain that the suite forks itself, and against a deployed network while
its operators roll the fork out.

The checks cover the chain and the software users reach it through: the toolkit, the
indexer, the proof servers, the wallet SDK and Midnight.js.

## Layout

```
lib/        target selection, results tables, RPC/indexer/toolkit helpers, dApps, clients
checks/     the checks, by results table, for either target
local-env/  start the ledger-8 chain, fork it, re-sync tests
network/    env files per network, preflight, stage runner
dapps/      Compact sources and toolkit-js config templates
clients/    wallet SDK, Midnight.js and polkadot-js checks (npm workspace)
report/     the report generator
```

## Prerequisites

- Docker, with access to `ghcr.io/midnight-ntwrk` images.
- bash 4+ first in `PATH` (macOS: `brew install bash`), `jq` 1.7+, `curl`, `python3`, `xxd`,
  `git`, `openssl`, Node.js 22 with npm.
- Optional: `gh`, to download the release wasm (without it `HF-WASM-1` takes the node image's
  wasm and WARNs that the srtool digest was not checked).
- For local-env: what [local-environment](../../../local-environment/README.md) needs, and
  about 16 GB of memory for Docker.

## Running on local-env

```sh
tests/hardfork/ledger-8-to-9/local-env/run.sh               # every phase, about an hour
tests/hardfork/ledger-8-to-9/local-env/run.sh l9 features   # some phases, on the running chain
```

| Phase | What it does | Tables |
|---|---|---|
| `env` | fresh local-env on `node-1.0.400`, with the indexer | - |
| `l8` | the ledger-8 baseline the fork must preserve | L8 |
| `clients-l8` | wallet SDK, Midnight.js and proof servers on ledger 8 | CLI-L8 |
| `fork` | binary waves with a smoke after each, the pre-fork snapshot, `set_code` | HF, WAVE-n, SNAP |
| `snapdiff` | the snapshot diffed on ledger 9 | SNAPDIFF |
| `l9` | what the fork preserved | L9 |
| `features` | what ledger 9 adds | FEAT |
| `clients-l9` | the client stack on ledger 9 | CLI-L9 |
| `resync-validator` | a wiped validator re-syncs through the fork | HF11 |
| `resync-indexer` | a wiped indexer re-syncs; the released indexer against this node | HF12 |
| `security` | security regressions and release completeness | SEC |
| `safe-mode` | SafeMode and its governance drill (runs last: after the drill the toolkit can no longer replay the chain) | SAFE |

A phase with failing checks is recorded and the run goes on. If a phase can't run, or `env`
or `fork` fails, the run stops and leaves the environment up. `HF_QUIET=1` prints only the
per-phase summaries.

`MANUAL_UPGRADE=1` stops the `fork` phase before `set_code` and prints the wasm, its code hash
and the governance calls to submit in Polkadot.js Apps. HF-UPG-1..3 are then not recorded; once
the upgrade is enacted, continue with the phases the runner names, e.g.
`run.sh snapdiff l9 features clients-l9 ...`, which find the fork block themselves.

## Running against a network

```sh
export SEEDS_FILE=~/secrets/devnet-hf.seeds
tests/hardfork/ledger-8-to-9/network/run.sh devnet preflight
```

The network runs in stages, one after each step of the operators' rollout. See
[network/README.md](network/README.md) for the walkthrough.

## The checks

Each table names the script that runs it. Local and network runs use the same check IDs; the
last two columns say where each check runs.

### Preflight (`network/preflight.sh`, PRE)

| IDs | Checks | Local | Network |
|---|---|---|---|
| PRE-1..7 | RPC, indexer, genesis and toolkit caches, toolkit images, both caches warmed for the test wallets, proof servers and clients | - | yes |

### Ledger-8 baseline (`checks/l8_baseline.sh`, L8)

| IDs | Checks | Local | Network |
|---|---|---|---|
| L8-VER-1..4 | ledger `=8.1.3`, spec `1000300`, toolkit and compactc versions, genesis | yes | yes |
| L8-NET-1 | block production and finality | yes | yes |
| L8-FUND-1, L8-DUST-1..2 | NIGHT and DUST of the test wallets; DUST generating | yes | yes |
| L8-TX-1..5 | unshielded, shielded, mixed, multi-destination, second-wallet transfers | yes | yes |
| L8-CS-1..4 | built-in contracts A and B, a committee rotation, semantic state recorded | yes | yes |
| L8-DAPP-1..3 | counter and bboard deployed and called, micro-dao deployed (best effort); semantic state recorded | yes | yes |
| L8-PARAM-1, L8-FETCH-1 | ledger parameters; every block fetched | yes | yes |
| L8-IDX-1..4 | indexer synced, contracts indexed, protocolVersion and DUST tree, every tx in a block | yes | yes |
| L8-ECO-1 | faucet and explorer | - | yes |
| HF-PRE-1..3 | an unsent ledger-8 tx the node validates; the ledger-9 toolkit on this chain; the toolkit refusing ECDSA on ledger 8 (client-side: ledger 8 can't encode ECDSA) | yes | yes |

### The fork

| IDs | Checks | Local | Network |
|---|---|---|---|
| HF-SWAP-1..6, HF-WAVE-n (`local-env/fork.sh`) | binary waves, postgres recreated with TLS, runtime unchanged, indexer running, agreement | yes | operators |
| WAVE-1..8 (`checks/wave_smoke.sh`, one WAVE-n table per wave) | per wave: spec, finality, versions, agreement, a transfer, a contract call, a dApp call, the indexer | yes | yes |
| HF-WASM-1, HF-GATE-1..2, HF-UPG-1..3 | release wasm vs its srtool digest, proof server and indexer ready, snapshot, `set_code`, fork block, agreement | yes | operators |
| SNAP-*, SD-* (`checks/snapshot.sh`) | state roots, parameters, NIGHT pools (24B total), wallets, contract data and transactions before the fork, read back after it | yes | yes |

### Preserved by the fork (`checks/l9_preserved.sh`, L9)

| IDs | Checks | Local | Network |
|---|---|---|---|
| L9-VER-1..4, L9-NET-1 | ledger-9 versions on every node and the toolkit; finality | yes | yes |
| L9-DUST-1..6 | cNIGHT replay migration, native DUST reset by design, a cNIGHT-backed source kept, self-funded re-registration, generation restarts, a held registration generates from its block's time | yes (no cNIGHT) | yes (L9-DUST-1 with `KUBE_CONTEXT`) |
| L9-STATE-1..4 | pre-fork contracts: reads, writes, data unchanged across the fork block, committee rotation | yes | yes |
| L9-TX-1..4 | the transfer matrix from pre-fork funds | yes | yes |
| L9-HF08-1..3 | the node rejects the saved ledger-8 tx (`1010`, Custom error 1) and a corrupted ledger-9 tx; seed 3 unspent; the chain unharmed | yes | yes |
| L9-NEW-1, L9-REWARDS-1 | a fresh contract; claim-rewards | yes | yes |
| L9-DAPP-1..3 | pre-fork dApps, with original and with recompiled artefacts | yes | yes |
| L9-PARAM-1..2, L9-FETCH-1 | static parameters unchanged, `min_block_price` floor; every block fetched across the fork | yes | yes |
| L9-AGREE-1..5 | agreement from the fork block to the tip, same migration cost on every validator | yes | AGREE-1..2 with `EXTRA_RPC_NODES`, AGREE-5 with `KUBE_CONTEXT` |
| L9-IDX-1..5 | the indexer crossed, contracts queryable, protocolVersion and DUST tree, node and indexer agree, every tx of the phase in a block | yes | yes |

### Added by ledger 9 (`checks/l9_features.sh`, FEAT)

| IDs | Checks | Local | Network |
|---|---|---|---|
| FEAT-ECDSA-1..3 | an ECDSA NIGHT identity spending; an ECDSA committee; rotation to a mixed committee | yes | yes |
| FEAT-EVT-1..2 | contract events emitted and served by the indexer, with filters and per-call attribution | yes | yes |
| FEAT-ZKIR3-1..2 | a keccak256 circuit for ZKIR v3 proved and verified; the embedded prover | yes | yes |
| FEAT-CCC-1..2 | a cross-contract call | yes | yes |
| FEAT-BRIDGE-1..2 | the bridge pallet configured; a live cNIGHT -> NIGHT transfer | pallet only | yes (`BRIDGE_TEST=1`) |
| FEAT-RSA-1, FEAT-USDCX-1 | not covered yet; listed so the gap stays visible | skip | skip |

### Client stack (`checks/client_stack.sh`, CLI-L8 / CLI-L9)

| IDs | Checks | Ledger 8 | Ledger 9 |
|---|---|---|---|
| C1-C3 | proof servers of the right family, the indexer serves the wallet's fields, pinned client versions | yes | yes |
| C4-C6 | the SDK's fork schedule, a wallet sync, an SDK transfer proved at the era's proof server | yes | yes |
| C7-C9 | a transfer proved at the ledger-8 proof server refused; SDK DUST re-registration; a restore across the fork | - | yes |
| C10-C11 | Midnight.js reads the head era; the toolkit proving at the proof server | yes | yes |
| C12 | Midnight.js refuses a retained-era deploy (by design); dApps deploy through the toolkit before the fork | yes | - |
| C13-C16 | pre-fork dApps through Midnight.js with ledger-8 artefacts; events; decoded state of pre-fork contracts; a ledger-9 dApp | - | yes |

All of these run on both targets.

### Resilience and security

| IDs | Checks | Local | Network |
|---|---|---|---|
| HF11-1..2 (`local-env/resync_validator.sh`) | a wiped validator re-syncs through the fork and agrees | yes | - |
| HF12-1..4 (`local-env/resync_indexer.sh`) | a wiped indexer re-syncs; a wallet sees the same NIGHT; the released indexer against this node | yes | - |
| SAFE-1..5 (`checks/safe_mode.sh`) | SafeMode present and idle; the fork window clean; the governance drill; toolkit and indexer after it | yes | drill opt-in |
| SEC-PROV-1..3 (`checks/security.sh`) | the commits in `checks/required-commits.tsv` are in the node tags and the indexer source | yes | yes |
| SEC-PIN-*, SEC-IMG-*, SEC-IDX-1, SEC-NODE-1, SEC-CLI-* | ledger and polkadot-sdk pins, no gdb, npm tar version, bounded indexer queries, unclean-shutdown recovery, client ledger versions | yes | yes (no SIGKILL) |

## Results

Output goes to `target/hardfork-8-to-9/local/` or `target/hardfork-8-to-9/network-<name>/`:

| Path | Holds |
|---|---|
| `results/SUMMARY.md`, `results/<table>.tsv` | every check |
| `report/REPORT.md`, `report/report.html` | the report |
| `evidence/` | evidence per check |
| `state/` | what later phases compare against |
| `runs/<time>/` (local; `runs/latest` is the newest) or `runs/<time>-<stage>/` (network) | logs per run |

| Status | Meaning |
|---|---|
| PASS | the check held |
| FAIL | the check didn't hold |
| WARN | something unexpected, or a limitation not filed upstream; the detail says which |
| SKIP | not run; the detail says why |
| KNOWN | a limitation filed upstream; the detail links the issue |

The checks fail closed:
- a second verdict in one check is recorded as an extra FAIL row;
- a script that dies mid-table gets a `<TABLE>-ABORTED` FAIL row;
- agreement needs at least `MIN_AGREE_NODES` (default 2) nodes that answered;
- the fork height is used only once the old spec is seen at the block before it.

The runners exit non-zero on any FAIL. `STRICT_KNOWN=1` turns KNOWN into FAIL, for a release
gate. `T_FAIL_FAST=1` stops a table at its first failure, and `T_APPEND=1` adds to a table
instead of starting it over. The report counts a check by its worst row, so an appended PASS
doesn't hide an earlier FAIL.

KNOWN is recorded only when the evidence matches the issue:
- node#1969: pre-fork dApps can't be called through toolkit-js with their ledger-8 artefacts
  (closed as intended; recompiled artefacts work, and Midnight.js calls the originals). Only
  the `Version mismatch` or `IncompatibleCodegen` error counts.
- indexer#1605: the indexer serves the pre-fork contract-state encoding for contracts untouched
  since the fork. Only when its bytes equal the node's at the block before the fork.
- indexer#1604: the indexer counts a transaction the node rejected in safe mode (node#2205 for
  the pool side).

Limitations not filed upstream, and other unexpected results, are WARN: the toolkit's embedded
prover is V2-only (FEAT-ZKIR3-2), toolkit-js can't run a cross-contract call (FEAT-CCC-2), the
toolkit can't replay past a filtered extrinsic (SAFE-3, SAFE-4), SDK DUST re-registration of
UTxOs the toolkit already registered (C8), the released indexer against this node (HF12-4), and
Midnight.js bundling ledger builds other than the chain's (SEC-CLI-2).

## Reports

Each run ends by writing `report/REPORT.md` and a single-file `report/report.html`: the
verdict, the environment, coverage of `report/test-plan.tsv`, failures and known issues with
links, what didn't run, and every check. To regenerate it from a results directory:

```sh
python3 tests/hardfork/ledger-8-to-9/report/generate.py target/hardfork-8-to-9/network-qanet/results \
    --out /tmp/qanet-report --tested-by "<team>" --notes notes.md \
    --explorer 'https://polkadot.js.org/apps/?rpc=wss%3A%2F%2Frpc.qanet.midnight.network#/explorer/query/{height}'
```

The headline says the fork completed only with evidence of the fork block (HF-UPG-2, or, where it
did not run (a network, `MANUAL_UPGRADE=1`), a verified `FORK_HEIGHT`), at least one L9 PASS and no FAIL; otherwise it says what
the results do show. `--notes` adds a hand-written section. The runners pass `REPORT_NOTES` as `--notes` and
`REPORT_TESTED_BY` as `--tested-by`; the explorer link comes from the network env's
`EXPLORER_BLOCK_URL`.

## Versions and knobs

The versions under test are defaults in `lib/common.sh`; each can be overridden from the
environment:

| Variable | Default | Meaning |
|---|---|---|
| `L8_REF` | `node-1.0.400` | the ledger-8 release |
| `L9_REF` | `node-2.1.0-rc.4` | the ledger-9 release; a `node-*` tag uses release images, any other ref (a branch, a commit) the CI images of its checkout; on a network such a ref also needs `L9_NODE_IMAGE` and `L9_TOOLKIT_IMAGE`; for a pull request set `L9_REF=` (empty) and `L9_PR=<number>` |
| `INDEXER_TAG` | `4.4.0-rc.6-d543f011` | the indexer paired with the ledger-9 node |
| `INDEXER_RELEASED_TAG` | `4.4.0-rc.5` | the latest published indexer, for the HF12-4 pairing check |
| `PS_L8_TAG`, `PS_L9_TAG` | `8.1.3`, `9.0.0-rc.7` | the proof servers |
| `L8_EXPECTED_*`, `L9_EXPECTED_*` | | the versions the checks expect |

Client versions are pinned in `clients/package.json`. `INDEXER_TAG` must be built against the
ledger-9 node under test: an indexer built for another node release stops at the fork block.

Other knobs: `HF_WAVES` (local waves, default `1 2,3,4,5`), `MANUAL_UPGRADE=1` (see
[Running on local-env](#running-on-local-env)), `RUNTIME_WASM_SOURCE=image`, `TOOLKIT_L8_BIN` /
`TOOLKIT_L9_BIN` (native toolkit builds), `PROOF_SERVER_L8` / `PROOF_SERVER_L9`, `ECDSA_SEED`
(default seed 4), `FEAT_ONLY` (a subset of `ECDSA,EVT,ZKIR3,CCC,BRIDGE,SCOPE`), `RUN_FULL_FETCH=1`,
`DUST_HOLD_SECS` (how long L9-DUST-6 holds a registration before sending it, default 60),
`SEC_SIGKILL=0`, `INDEXER_PAIRING_CHECK=0`, `SEC_REQUIRED_COMMITS_FILE`, `GITHUB_TOKEN` (for the
GitHub API calls of SEC-PROV-3), `WAVE_EXPECTED_L9`, `MJS_RETAINED_COMPACTC` /
`MJS_CURRENT_COMPACTC` (default `0.31.1` / `0.34.0`), `BRIDGE_AMOUNT` / `BRIDGE_WAIT_S`,
`HF_WORK_DIR`, `MIN_AGREE_NODES`.

Timeouts: `TX_WATCHDOG_SECS` (a toolkit transaction, default 900; the container is killed when
it runs out), `MN_TX_TIMEOUT_SECS` (a client transaction, default 600) and `MN_TIMEOUT_SECS`
(a whole client command, default 1800, 3600 on networks); `MN_SYNC_TIMEOUT_MS` (a wallet sync,
900000 on networks).

## CI

The local-env run needs published images of both node releases and an indexer build, about
16 GB of memory and about an hour, so it isn't a pull-request check. It fits a scheduled or
manually triggered workflow on a large self-hosted runner, for example nightly on
`release/node-*` with `L9_REF` set to the branch head. Upload `results/` and `report.html` as
artefacts.
