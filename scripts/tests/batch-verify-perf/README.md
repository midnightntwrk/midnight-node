# Batch-verification block-import performance harness

A Docker-based A/B benchmark for the batch ZK-proof verification added in this
PR. It measures how much faster a node **imports** a proof-heavy chain when
`batch_verify_block_import` is on vs off.

## Why block import (and why two nodes)

Batch verification hooks two ingress points: the mempool and block import. The
block-import path (`BatchVerifyBlockImport`, wired at `ImportQueueParams.block_import`
in `node/src/service.rs`) only fires on blocks a node **did not author** —
authored blocks carry `StateAction::ApplyChanges` and skip it. So to exercise
it we need a node that *imports* blocks it didn't make.

That gives a clean, deterministic, single-workload benchmark:

```
                 prime.sh (once)                    benchmark.sh (repeatable)
   ┌──────────────────────────────┐        ┌───────────────────────────────────┐
   │  dev authority                │        │  producer            node-under-test│
   │  (CFG_PRESET=dev)             │        │  (restored archive)  (fresh, wiped) │
   │  ── toolkit floods proof-txs ─┼──▶ tar │       │  ── p2p full-sync ──▶ │      │
   │  archive /node/chain          │  .tar  │  serves blocks 1..N   imports+verifies│
   └──────────────────────────────┘  .gz   └───────────────────────────────────┘
                                                        run twice: flag off, then on
```

Both benchmark runs execute the same blocks and verify the same proofs; only
the batching differs, so the OFF−ON delta is the batch-verify speedup.

## Prerequisites

Build the node and toolkit images **from this branch** (they must contain the
batch-verification code). A host-built binary can't be dropped into the image —
the amazonlinux base has an older glibc than a typical dev box.

```bash
earthly +node-image      # e.g. loads localhost/midnight-node:<tag>
earthly +toolkit-image
```

Use whatever tags those produce as `NODE_IMAGE` / `TOOLKIT_IMAGE` below. Docker
(with a working daemon) and `curl` must be on PATH.

## Usage

```bash
cd scripts/tests/batch-verify-perf

# Phase 1 — build the archive once (proving is up-front; this takes a while):
./prime.sh <NODE_IMAGE> <TOOLKIT_IMAGE>
#   -> artifacts/chain-archive.tar.gz  (+ .meta with the height reached)

# Phase 2 — A/B benchmark, rerun as often as you like:
./benchmark.sh <NODE_IMAGE>
```

Or via `just`:

```bash
just batch-verify-perf-prime <NODE_IMAGE> <TOOLKIT_IMAGE>
just batch-verify-perf-bench <NODE_IMAGE>
```

### Benchmarking a locally-built node (no image rebuild)

`benchmark.sh` picks its run mode the same way `toolkit-tokens-minter-e2e.sh`
does: pass a node image and it runs containers; pass **nothing** and it runs a
locally-built binary as **host processes** (producer + syncer on localhost),
against the same existing archive. This is the fast inner loop when iterating on
node-side changes — build once, benchmark without waiting on a CI image.

```bash
cargo build --release                 # or: cargo build  +  NODE_BIN=target/debug/midnight-node
cd scripts/tests/batch-verify-perf
./benchmark.sh                         # local mode — uses target/release/midnight-node
NODE_BIN=/path/to/midnight-node ./benchmark.sh   # explicit binary
```

Notes for local mode:

- The image base (amazonlinux 2023, glibc 2.34) is older than a typical dev
  host, so a freshly-built host binary can't run *inside* the image — hence host
  processes rather than a layered image.
- The binary runs with the repo root as its CWD because the `dev` preset
  (`res/cfg/dev.toml`) references its chainspec/genesis/mock files by relative
  path.
- It **reuses the existing archive**, whose genesis must match the local
  binary's `dev` chainspec. If the syncer can't find the producer's chain,
  re-prime with a matching build.
- Host base-paths and logs land under `artifacts/local/` (`producer.log`,
  `syncer.log`) for debugging.

### Example output

```
═══════════════ batch-verify block-import benchmark ═══════════════
node image             : localhost/midnight-node:latest
blocks synced          : 33
OFF (inline verify)    : 214s
ON  (batch verify)     : 92s
delta (off - on)       : 122s
speedup (off / on)     : 2.33x

ON batch coverage      : batches=33 txs_total=812 (chain load proof-txs=800), avg 24.6 txs/batch
ON crypto time         : 41.2 ms/batch  (1.360s total over 33 batches)

--- per-midnight-tx proof verification (crypto, OFF inline vs ON batched) ---
  OFF inline           :   50.000 ms/tx   (812 txs, well_formed WITH proofs)
  ON  batched          :   12.000 ms/tx   (812 txs = 10.000 crypto + 2.000 prep)
  full-verify speedup  : 4.17x   (50.000 -> 12.000 ms/tx)
  crypto-only speedup  : 4.80x   (48.000 -> 10.000 ms/tx)
...
```

**Wall-clock vs per-tx.** The top block is end-to-end full-sync time; at dev
scale it is dominated by peer-connect + the 6 s AURA cadence + DB writes, so it
often can't resolve the verification delta. The **per-midnight-tx** block is the
tx-granular signal — it reads the ledger-side `ledger_proof_verify_*` metrics
(labelled by `mode`), which record the ZK crypto directly on both runs:

- **OFF run** → `mode="inline"`: one per-tx `well_formed` **with proofs** (cold
  proof cache — the OFF/inline path).
- **ON run** → `mode="batch"`: one aggregate `batch_verify_proofs` call, plus
  `mode="batch_prep"`: the per-tx `well_formed` **without proofs** (the non-crypto
  work both paths pay).

Per-tx cost = `_sum / _txs_total`. `full-verify` compares OFF's fused
`well_formed` against ON's `batch + prep`; `crypto-only` subtracts the shared
non-crypto cost to isolate the ZK verification speedup. This needs a node built
from this branch (the metrics don't exist in older images).

The `midnight_batch_verify_*` counters confirm the batch path actually engaged
on the ON run. If `batches_total` is 0, the node silently fell back to inline
verification (e.g. it couldn't build the native block context) — the timing is
then meaningless; check the syncer logs (`docker logs bv-syncer`).

## The prime workload

`batch-single-tx` builds each transfer independently and doesn't reserve coins
between the concurrent builds, so two specs sharing a source would select the
same coin and collide. It therefore needs **one distinct, pre-funded source per
transfer**. `prime.sh` provisions that in two steps:

1. **fan-out** — one or more `single-tx` calls split the genesis shielded
   balance into `LOAD_TXS` coins across `LOAD_TXS` derived wallets. This is the
   "prime the chain with lots of outputs" step.
2. **load** — `batch-single-tx` builds `LOAD_TXS` independent shielded
   transfers, each `source_seed` spending its own fanned-out coin, with the
   single genesis wallet as `funding_seed` paying every fee. DUST is
   contention-free by design, so one funder covers all fees; only the *coins*
   have to be fanned out first.

Both steps submit to the priming node, so all the proof-txs end up in the
archived chain.

## Tuning

All knobs are environment variables (see `lib.sh` for the full list and
defaults):

| Variable | Meaning | Default |
|---|---|---|
| `LOAD_TXS` | number of independent proof-bearing txs | `150` |
| `FANOUT_CHUNK` | outputs per fan-out `single-tx` (tx-size cap) | `25` |
| `FAN_AMOUNT` / `SEND_AMOUNT` | shielded coin value seeded / moved | `100` / `100` |
| `LOAD_RATE` | load submit rate (txs/sec) | `40` |
| `SHIELDED` | `1` = shielded (zswap) proofs; `0` = unshielded (no proofs) | `1` |
| `SYNC_TIMEOUT_SECS` / `STALL_TIMEOUT_SECS` | benchmark watchdogs | `1800` / `240` |
| `BATCH_VERIFY_MAX_BATCH_SIZE` etc. | forwarded to the syncer when set | (node defaults) |

A bigger, prove-heavier chain shows a larger absolute gap — scale `LOAD_TXS`
(every tx is proved up-front, so prime time grows with it). Keep `SHIELDED=1`:
the batching accelerates ZK-proof verification, so the workload has to carry
proofs.

## How it works (implementation notes)

- **Config flags** flip via env: `-e BATCH_VERIFY_BLOCK_IMPORT=true` overrides
  the `dev` preset (env sits above the preset in the node's config precedence).
- **The syncer** runs `CFG_PRESET=dev` (to keep the mock main-chain-follower
  config) but its run args are replaced with an explicit
  `--chain dev --node-key … --bootnodes …` set. `--chain dev` selects the same
  genesis as `--dev` **without** injecting Alice keys or force-authoring, so the
  syncer never authors — it only imports. Its own node key keeps it from
  colliding with the producer on the network.
- **The bootnode multiaddr** is derived from the producer's real peer id
  (grepped from its startup log), not hard-coded.
- **The archive** is a gzip of the primed node's `base_path`
  (`/node/chain` = substrate DB + `ledger_storage`), taken after a graceful
  stop. It lives under `artifacts/` (git-ignored).

## Cleanup

Both scripts remove their own containers on exit (including on error). To reset
everything, including the reusable archive:

```bash
docker rm -f bv-prime bv-producer bv-syncer 2>/dev/null
docker volume rm -f bv-prime-data bv-producer-data bv-syncer-data 2>/dev/null
docker network rm batch-verify-net 2>/dev/null
rm -rf artifacts
```
