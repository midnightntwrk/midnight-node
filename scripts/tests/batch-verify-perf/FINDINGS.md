# Batch-verify block-import benchmark — findings

Run against `ghcr.io/midnight-ntwrk/midnight-node[-toolkit]:2.0.0-1d675be7a932`
(node 2.0.0, ledger 9), dev chain, single host, all containers on one docker
network. See `README.md` for how to reproduce.

## TL;DR

The harness works end-to-end and the block-import batch path **provably engages**
(100% of proof-txs batched, no inline fallback). But at dev scale the A/B
**wall-clock delta is 0 within noise**, and the metrics explain why: the batch
sizes are tiny (≤5 txs), which is batch verification's weak regime. The batch
size is capped by how many transactions one funding wallet can fee in a single
`batch-single-tx` snapshot — the **DUST-output count (5 for genesis)** — which is
the crux limiting both workload generation and the achievable batch size.

## What the wall-clock said (120 proof-txs, 148 blocks, 4 repeats)

```
OFF (inline verify)  : min=4.3s  [4.3 6.3 4.3 4.3]
ON  (batch verify)   : min=4.3s  [6.3 4.3 4.3 6.3]
delta 0.0s   speedup 1.00x
```

A fresh full sync of 148 blocks is ~4.3s, and that floor is **peer connection +
6 s AURA block cadence**, not verification. The same OFF config ranged 4.3→10.3 s
across runs, so the verification delta (sub-second for ~125 proof-txs) is far
below the noise floor. Conclusion: **this scale cannot resolve the verification
delta** — not evidence for or against batch verification's value.

## What the metrics said (scraped from the syncer's `:9615/metrics`)

```
midnight_batch_verify_batches_total{outcome="success"} 31
midnight_batch_verify_txs_total                        125     (= 120 load + 5 fan-out)
midnight_batch_verify_batch_size  sum=125 count=31  buckets: le=1→6, le=2.5→7, le=5→31, le=10→31
midnight_batch_verify_duration_seconds  count=0            (empty)
midnight_batch_verify_fallback_total                   0    (always 0 on a syncer — see below)
```

1. **Coverage / correctness — 100%.** `txs_total = 125` equals every proof-tx in
   the chain and `batches_total = 31` equals the number of proof-bearing blocks,
   so the block-import wrapper batch-verified **every** proof-tx; none skipped to
   the inline path. `fallback_total = 0` is *not* the evidence here (see #3).

2. **Batches are tiny — all ≤5 txs.** The `batch_size` histogram tops out in the
   `le=5` bucket (all 31 batches ≤5, avg ~4). This is a direct consequence of the
   DUST-output limit: `batch-single-tx` fees every tx from the single genesis
   wallet, and genesis has 5 DUST generation outputs, so at most 5 proof-txs land
   per block. Batch verification amortizes a fixed per-call cost across the batch;
   at batch size ≤5 there is almost nothing to amortize, which is exactly why the
   wall-clock is flat. Batch verification is expected to pay off only when blocks
   carry *many* proof-txs.
   - Note: a fan-out tx with 25 shielded outputs still counts as **1 tx** in
     `batch_size` — the aggregate call batches across *transactions*, so dense
     multi-output txs do not enlarge the batch; more *txs per block* would.

3. **Crypto-time instrumentation (now wired for both paths).** Originally
   `observe_batch_duration` (→ `duration_seconds`) was called only on the mempool
   path, so block-import `duration_seconds` was always empty. **Fixed:** the timer
   now wraps the aggregate crypto call (`batch_verify_transactions`) *inside*
   `BatchVerifier::batch_verify` (node/src/batch_verify.rs), which both ingress
   paths call — so `duration_seconds` records the per-batch crypto time for block
   import too (and the mempool's redundant external timer was removed to avoid
   double-counting). `benchmark.sh` reports it as "ON crypto time … ms/batch".
   This measures verification directly, sidestepping the sync-time noise — but it
   requires a node image **rebuilt from this branch** (the `2.0.0-…` image predates
   the change and still records nothing).
   - Still open (not done): `fallback_total` remains mempool-only, and a
     block-import skip (`BatchVerifyError::Unavailable`) returns `Ok` with no
     counter — so block-import skip-rate is still not observable via metrics. Use
     `txs_total` vs the chain's proof-tx count as the coverage signal.

4. **Per-midnight-tx crypto metrics (new — the fix for the OFF-side gap).**
   `midnight_batch_verify_duration_seconds` only records on the ON path, so it
   has nothing to diff against. The ledger now records the ZK crypto on **both**
   paths at transaction granularity, via `ledger_proof_verify_duration_seconds` /
   `ledger_proof_verify_txs_total` (label `mode`), in `LedgerMetrics`:
   - `mode="inline"` — per-tx `well_formed` **with proofs** in
     `get_verified_transaction` (the OFF/cold-cache path).
   - `mode="batch"` — the aggregate `batch_verify_proofs` call (ON crypto).
   - `mode="batch_prep"` — per-tx `well_formed` **without proofs** on the batch
     path (the non-crypto work both paths pay).
   Both families land in the same registry (the batch-path externalities and the
   runtime execution path share one `LedgerMetricsExt`). `benchmark.sh` scrapes
   them from both runs and reports per-tx `full-verify` and `crypto-only`
   speedups (per-tx = `_sum / _txs_total`). This sidesteps the wall-clock noise
   floor: even when the sync-time delta is 0 within noise, the crypto delta is
   measured directly. The OFF verification path is unchanged — the inline
   `well_formed` is only wrapped in a timer.

## The DUST-output ceiling (verified)

`batch-single-tx` builds every spec against one static snapshot with no coin/DUST
reservation between builds, so it needs one distinct pre-funded source per tx and
fees them all from `funding_seed`. On a fresh dev chain:

- Genesis DUST is **not** scarce: `dust-balance` shows total = capacity = 1.25e24
  (~8e9 txs' worth). But it sits in exactly **5 generation outputs**.
- A single `batch-single-tx` invocation therefore tops out at **5 successes**;
  the rest fail `Insufficient DUST (trying to spend X, need X more)`. Verified
  deterministic — `--concurrency 1` gives the identical 5-succeeded/N-failed
  split, so it is the funder's output count, not a race.
- **Workaround (used by prime.sh):** chunk the load into groups of ≤5, one
  invocation per chunk, re-fetching between (each spent output leaves a change
  output). Scales to any N; balance is never the limit. The ledger's DUST is
  contention-free — this is purely a toolkit static-snapshot-build limitation.

The same ceiling caps the achievable per-block batch size at ~5, which is why the
benchmark can't reach batch verification's favourable regime with a single funder.

## To get a measurable signal (not yet run)

Verification has to dominate sync time, i.e. blocks must carry *many* proof-txs:

- **More DUST outputs → bigger batches.** Split genesis NIGHT into many UTXOs (each
  backs a DUST output) so one snapshot can fee ≫5 txs/block. New outputs need to
  age (grace period 10800 s) before their DUST is spendable, so this is a slow
  setup step — but it is the direct lever on batch size.
- **Much larger N** so cumulative verification time rises above the ~4 s connection
  floor (slow to prime: ~24 s per 5-tx chunk).
- **Measure the mempool path instead**, which already records `duration_seconds`:
  submit txs to a `batch_verify_mempool=true` node and read duration vs batch_size
  directly — a noise-free crypto-time curve, no sync involved. (The block-import
  path was chosen for this harness; this is the metric-driven alternative.)

## Bugs fixed while validating

- `prime.sh`: waited on best-block, but the toolkit fetches the *finalized* chain →
  `OnlyGenesisFinalized`; now waits for finality.
- Toolkit `/out` mount left the host dir owned by uid 10001 → `.meta` write denied;
  fixed with `-e RESTORE_OWNER`. Persisted `/.cache` volume so ZK proving keys
  download once, not per chunk.
- Metric scrape assumed `name<space>value`; the metrics carry labels
  (`{outcome=...,chain="undeployed"}`) → nothing matched. Fixed.
- Benchmark reused a fixed syncer node-key → rapid reconnect cycles stalled at
  "0 peers"; now a fresh random key per run.
- Benchmark verdict keyed off `fallback_total` (mempool-only, always 0 on a
  syncer); now keyed off `txs_total` vs the chain's proof-tx count.
