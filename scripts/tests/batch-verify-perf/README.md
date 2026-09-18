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

**Or skip the images entirely.** `prime-local.sh` builds the same archive from locally-built
binaries, so neither phase needs Docker:

```bash
cargo build --release -p midnight-node -p midnight-node-toolkit
just seed-zk-keys                      # this branch's proving keys are not published

cd scripts/tests/batch-verify-perf
./prime-local.sh 224                   # ~18 min for 224 proof-txs
./benchmark.sh                         # no args = local mode
```

That is usually the faster loop: building the two images requires the branch's ledger crates to be
published as an isolate first (see `Batch-Verification-Notes.md`), whereas the binaries are already
on disk.

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

## Counting proof re-verifications (`proof-reverification.sh`, no Docker)

`benchmark.sh` answers "how much faster is import with batching on?". A different
question is "how many times does one transaction's proofs get verified at all?" —
which is what the batch work exists to reduce. `proof-reverification.sh` measures
that directly, on a single authoring node, with no images and no archive:

```bash
just batch-verify-perf-reverify 3        # or: ./proof-reverification.sh 3
```

It starts a dev node, derives a destination, submits N freshly-proved shielded
transfers over RPC, and diffs the ledger's `ledger_proof_verify_txs_total`
counters across their `mode` labels:

- `inline_mempool` — proofs verified while admitting the tx to the pool
- `inline` — proofs verified at `pre_dispatch`, during block authoring/execution
- `batch` — proofs verified in an aggregate call at a batch ingress point

A transaction counted under **both** inline labels had its proofs verified twice
on one node. Measured on the shipped defaults:

```
batch_verify_mempool                : false
transactions submitted              : 3
mempool admission  (inline_mempool) : 3
block execution    (inline)         : 3
inline verifications per tx         : 2.00x
```

and with `BATCH_VERIFY_MEMPOOL=true`, `batch=3` with both inline counters at 0
(0.00x) — the proof cache removes both. Any node config can be forced through the
environment, e.g. `BATCH_VERIFY_MEMPOOL=true BATCH_VERIFY_WORKERS=1 ...`.

Prerequisites are host binaries rather than images — `cargo build --release -p
midnight-node -p midnight-node-toolkit` — plus locally compiled proving keys
(`just seed-zk-keys`), since this branch's `static/version` is not published. The
unit-test counterpart, which pins the same behaviour against a synthetic state, is
`proofs_are_reverified_when_a_transaction_reaches_a_new_block` in
`ledger/src/versions/common/mod.rs`.

## Bigger batches: a custom genesis

Batch verification amortises a fixed cost across the batch, so its benefit depends on how many
proof-txs share a block. On the stock `undeployed` genesis that number is capped at **5**, and the
cap has nothing to do with the node: `batch-single-tx` fees every transaction from the genesis
wallet, and the wallet's DUST sits in one generation output per genesis NIGHT UTXO — of which there
are five (`--unshielded-num-funding-outputs`, default 5). One invocation can therefore fee at most
five transactions, and batches stay around 4.

To lift it, generate a genesis with more NIGHT outputs and point the harness at it:

```bash
# 1. a genesis with 64 NIGHT outputs (keep shielded at 5 — see the size note below)
midnight-node-toolkit generate-genesis \
  --network undeployed --seeds-file seeds.json \
  --ledger-parameters-config res/dev/ledger-parameters-config.json \
  --cnight-generates-dust-config res/dev/cnight-config.json \
  --ics-config res/dev/ics-config.json \
  --reserve-config res/dev/reserve-config.json \
  --shielded-num-funding-outputs 5 \
  --unshielded-num-funding-outputs 64

# 2. turn it into a chainspec (see the `--dev` trap below for why this step exists)
CFG_PRESET=dev \
  CHAINSPEC_GENESIS_STATE=out/genesis_state_undeployed.mn \
  CHAINSPEC_GENESIS_BLOCK=out/genesis_block_undeployed.mn \
  midnight-node build-spec --raw > bench-spec.json

# 3. prime and benchmark against it
CHAIN=/abs/path/bench-spec.json LOAD_CHUNK=32 ./prime-local.sh 224
CHAIN=/abs/path/bench-spec.json ./benchmark.sh
```

Measured on this branch: batch size 3.9 -> 12.3, crypto-only speedup 1.75x -> ~1.9-2.05x. That is
close to the ceiling — per-tx batched cost asymptotes to the fit's slope (~1.64 ms against ~3.5 ms
inline, so ~2.1x), and batches of ~12 already capture most of it. Going further buys little.

**Two traps worth knowing.**

*`--dev` silently ignores the genesis config.* `Cfg::load_spec` maps chain id `"dev"` to a hardcoded
built-in spec; `chainspec_genesis_state` / `_block` are validated (a bad path still errors) but never
read. A node started with `--dev` therefore runs the committed genesis no matter what those are set
to. Only chain id `""` builds from them, which is what `build-spec` above uses, and a chainspec
*path* is what `--chain` needs afterwards. `--dev` also implies `--alice --force-authoring`, so an
authoring node on a custom chain has to spell those out — `authoring_chain_args` in `lib.sh` does
this, keyed off `CHAIN`. Verify you got the genesis you meant by diffing the `Initializing Genesis
block/state (state: 0x…)` line against a stock run.

*Genesis funding is bounded by the 1 MiB transaction limit.* 64 shielded **and** 64 unshielded
outputs overflows it (`TransactionTooLarge { tx_size: 1080862, limit: 1048576 }`). Shielded outputs
carry proofs and dominate the size; NIGHT outputs are cheap, and NIGHT is what backs DUST. Raise the
unshielded count only — the shielded coins are fanned out by the load step anyway.

## A fixture the benchmark can resolve

Wall-clock sync time is roughly `44 ms x blocks + 3.9 ms x proof-tx`, so how quickly an A/B
resolves depends on **proof-txs per block**. The default workload is sparse — 233 txs over 82
blocks, about 20% of sync time — and a real improvement sits close to the noise floor there: it
took 42 paired runs to establish a direction.

A denser chain fixes that:

```bash
CHAIN=/abs/path/bench-spec.json LOAD_CHUNK=64 FANOUT_CHUNK=100 ./prime-local.sh 512
CHAIN=/abs/path/bench-spec.json REPEATS=9 ./benchmark.sh
```

| | sparse (default) | dense |
|---|---|---|
| blocks / proof-txs | 82 / 233 | 129 / 518 |
| txs per block | 2.84 | 4.02 |
| txs per *populated* block | 12.3 | 22.5 |
| verification share of sync | ~20% | ~28% |
| verification saved per sync | 0.30 s | 0.79 s |
| paired runs to resolve | 42 (p = 0.0001) | **9, unanimous (p = 0.002)** |

**`FANOUT_CHUNK` is the counter-intuitive knob.** Raising `LOAD_TXS` alone makes density *worse*.
Every load tx needs its own coin, and fan-out runs one `single-tx` per chunk, each taking ~25 s —
during which the chain keeps minting empty 6-second blocks. At the default `FANOUT_CHUNK=25`,
512 coins cost 21 fan-out txs and roughly 87 near-empty blocks. Fat fan-out txs (100 outputs,
~44 s each) cost 6 txs and ~43 blocks instead. The load phase needs no such help: `batch-single-tx`
proves a whole chunk before submitting any of it, so a chunk lands in one or two blocks.

Density is ultimately capped by local proving throughput (~1 tx/s) against the 6-second slot, so
most blocks stay empty regardless. The gain comes from the *absolute* effect size growing —
0.30 s to 0.79 s — not from the share reaching a majority.

Two side effects of the dense fixture:

- Batches go from 12.3 to 22.5 txs, and because aggregate verification amortises a fixed cost,
  crypto-only speedup rises from ~2.03x to ~2.33x with no code change.
- Eight blocks close on `HitBlockWeightLimit` (max 39 extrinsics), where the sparse fixture never
  filled one. Closer to a loaded chain — and it does not change the block-capacity finding, since
  weight is declared before execution.

Keep the sparse archive if you want the old numbers reproducible:

```bash
cp artifacts/chain-archive.tar.gz artifacts/chain-archive-sparse.tar.gz
cp artifacts/chain-archive.meta   artifacts/chain-archive-sparse.meta
```

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
| `CHAIN` | chain id, or a path to a chainspec JSON (see "Bigger batches") | `dev` |
| `LOAD_CHUNK` | txs per `batch-single-tx` call; cap is the funder's DUST-output count | `5` |
| `BATCH_VERIFY_MAX_BATCH_SIZE` etc. | forwarded to the syncer when set | (node defaults) |
| `AB_ENV_VAR` | which node setting the A/B flips | `BATCH_VERIFY_BLOCK_IMPORT` |

A bigger, prove-heavier chain shows a larger absolute gap — scale `LOAD_TXS`
(every tx is proved up-front, so prime time grows with it). Keep `SHIELDED=1`:
the batching accelerates ZK-proof verification, so the workload has to carry
proofs.

## Measuring a second feature against an existing baseline

`AB_ENV_VAR` chooses which setting the two arms differ in; everything else is
held fixed and forwarded to both. That keeps the comparison *paired* when the
baseline itself already has a feature on — which matters more the smaller the
effect, since running two separate benchmarks and diffing their medians puts
machine drift straight back into the answer.

Verification lookahead against plain block-import batching:

```bash
AB_ENV_VAR=BATCH_VERIFY_LOOKAHEAD \
BATCH_VERIFY_BLOCK_IMPORT=true \
BATCH_VERIFY_LOOKAHEAD_BLOCKS=4 \
BATCH_VERIFY_LOOKAHEAD_WORKERS=2 \
REPEATS=15 ./benchmark.sh
```

Both arms then batch-verify, so neither emits `mode="inline"` samples and the
per-tx sections print "insufficient samples" — that is expected, not a fault.
Read the wall-clock pairing and the raw counters instead. The ones that say
whether the pipeline is actually working:

| Counter | Reading |
|---|---|
| `lookahead_jobs_total{outcome}` | `failure` means jobs concluded nothing — usually no usable reference state |
| `lookahead_blocks_total{disposition}` | `hit` is a block that consumed a result; `miss` verified itself anyway |
| `lookahead_wait_seconds_sum` | time imports spent *waiting*; compare against `midnight_batch_verify_duration_seconds_sum` on the OFF arm to see how much verification came off the critical path |

A run where jobs succeed but no block hits, or where every job fails instantly,
is the feature doing nothing while still burning CPU. Both have happened; both
are invisible in wall clock alone, which is why these counters exist.

## The mempool A/B (`mempool-prime.sh`, `mempool-benchmark.sh`)

Measures `BATCH_VERIFY_MEMPOOL` — the *admission* path — rather than block import. Two phases,
for the same reason the sync harness has two: proving is seconds per transaction against
milliseconds of validation, so it has to happen once, outside the timed region.

```bash
./mempool-prime.sh 144          # fan out, archive the state, build 144 proved txs (once)
REPEATS=9 ./mempool-benchmark.sh
```

`mempool-prime.sh` fans the genesis balance into N coins, **archives the chain at that point**, and
builds N transactions with the toolkit's `--dest-file` without submitting them.
`mempool-benchmark.sh` restores that exact state for every run and replays the same transactions,
interleaved and counterbalanced, sharing the paired statistics in `lib.sh`.

**Read the verification budget, not the wall clock.** Submission wall clock is bounded by the AURA
slot cadence and by how fast one connection can push; neither changes with batching, and the
samples visibly quantize to 6 s. The budget is:

| arm | cost |
|---|---|
| OFF | `ledger_proof_verify_duration_seconds{mode="inline_mempool"}` |
| ON | `midnight_batch_verify_prepare_duration_seconds` + `midnight_batch_verify_duration_seconds` |

The ON side **must** include `prepare_duration`. The incremental preparation is where the expensive
per-proof work went, and it has no ledger-side `mode=` counter — totalling only `batch` +
`batch_prep` omits roughly 3 ms/tx of 3.9 and overstates batching by about 4x. The report prints
that subset underneath, labelled, with what it alone would have claimed.

Also watch the transaction counts: the report warns when the arms verify different numbers. A run
where ON verifies more than it was given is doing redundant work, and a per-transaction ratio will
not show it.

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
