# Batch Proof Verification — Investigation Notes

Goal: change mempool behavior so Midnight transactions are proof-verified through a
queue + worker pool, instead of one at a time through the stock Substrate pool path.
These are notes from an initial investigation into how validation currently works and
where custom behavior can be wired in — no implementation yet.

## Current validation flow (top to bottom)

1. **`node/src/service.rs:389-401`** — stock pool is built with
   `sc_transaction_pool::Builder::new(...).build()`, then immediately wrapped:
   ```rust
   let transaction_pool = sc_transaction_pool::Builder::new(...).build();
   let transaction_pool = FilteringTransactionPool::new(tx_filter_config, transaction_pool, client.clone(), metrics);
   ```
2. **`node/src/filtering_pool.rs`** — existing custom wrapper (`FilteringTransactionPool`)
   implementing `TransactionPool` / `MaintainedTransactionPool` / `LocalTransactionPool` by
   delegating to an inner `TransactionPoolWrapper<Block, Client>`. It already intercepts
   `submit_at` / `submit_one` / `submit_and_watch` / `submit_local` to run a synchronous
   accept/reject check (`should_accept_extrinsic`, currently used to deny `deploy`/`maintain`
   ops) before forwarding. Config (`TxFilterConfig`) is wired from a CLI flag in
   `node/src/command.rs` (`run_midnight.filter_deploy_txs`).
3. **`runtime/src/lib.rs:1377-1385`** — `impl TaggedTransactionQueue<Block> for Runtime`, the
   runtime API the pool's `ChainApi` calls into per transaction
   (`Executive::validate_transaction`).
4. **`pallets/midnight/src/lib.rs:441-462`** (`validate_unsigned`) and **`:464-486`**
   (`pre_dispatch`) — Midnight's `ValidateUnsigned` impl for `send_mn_transaction`. Both
   eventually call into `LedgerApi::validate_transaction` /
   `LedgerApi::validate_guaranteed_execution`.
5. **`ledger/src/versions/common/mod.rs:570-621`** (`validate_transaction`, mempool path) and
   **`:631+`** (`validate_guaranteed_execution`, pre-dispatch path) — deserialize the tx,
   fetch ledger state, then call `do_validate_transaction` / `do_validate_guaranteed_execution`.
6. **`ledger/src/versions/common/mod.rs:934-976`** (`get_verified_transaction`) — on cache
   miss, calls `tx.0.well_formed(...)` at **line 959**. This is the actual ZK proof check.
7. `LedgerApi` itself (`ledger/src/host_api/ledger_{7,8,9}.rs`) is a **host function** —
   native Rust the WASM runtime calls out to, version-dispatched per ledger version
   (7/8/9, matching `onchain-runtime` / `onchain-runtime-ledger-8` / `-9`). The `node` crate
   already depends on `midnight_node_ledger` directly (see imports in `filtering_pool.rs`),
   so native ledger code is reachable from node-side Rust without going through the runtime.

## Validation caches (already exist)

Two process-global moka caches in `ledger/src/versions/common/mod.rs`:

- **Soft cache** (`SOFT_TX_VALIDATION_CACHE`, key: `SoftTxValidationKey { tx_hash }`) — used
  by `do_validate_transaction` (mempool path). A hit short-circuits `well_formed()` entirely.
  Invalidated on new block via `do_validate_guaranteed_execution`.
- **Strict cache** (`STRICT_TX_VALIDATION_CACHE`, key: `StrictTxValidationKey { state_hash, tx_hash }`)
  — used by `get_verified_transaction`, shared between the mempool path and the
  pre-dispatch/guaranteed-execution path. Note the key does **not** include `block_context`
  (e.g. `tblock`), so the two call sites already tolerate the small `tblock` difference between
  them (mempool validation pads `tblock` by a skipped-slots margin, see
  `pallets/midnight/src/lib.rs:449-459`; pre-dispatch does not).
- Both `tx_hash` values above are actually `tx_validation_cache_key(runtime_version, tx_bytes)`
  = `Twox128(runtime_version_le_bytes ++ tx_serialized)` (`ledger/.../mod.rs:913-916`) — i.e.
  the cache key already incorporates `runtime_version`, not just the raw tx hash.

## Pool concurrency model (checked against actual `polkadot-sdk` source, tag `polkadot-stable2603`)

Cached checkout used for reference: `~/.cargo/git/checkouts/polkadot-sdk-dee0edd6eefa0594/`.

- `graph::Pool::submit_at` (`graph/pool.rs:234-243`) validates every extrinsic in a submitted
  batch **concurrently** via `futures::future::join_all` (`graph/pool.rs:502-511`) — there is
  no passive FIFO queue at this layer; everything submitted starts validating immediately.
- The actual bottleneck is inside `FullChainApi` (`common/api.rs`): `validate_transaction`
  pushes a boxed future onto an `mpsc::channel(1)`, consumed by exactly **two hardcoded
  worker tasks** (`spawn_validation_pool_task`, `common/api.rs:67-123`, spawned at
  `common/api.rs:152-167`, named `transaction-pool-task-0`/`-1`). Two priority lanes exist
  ("normal" new submissions vs "maintained" revalidation) but both share the same 2 workers.
  This worker count is a hardcoded constant, not configurable via `TransactionPoolOptions` or
  CLI.
- `TransactionPoolWrapper` (`transaction_pool_wrapper.rs`) only exposes the public
  `TransactionPool` trait — there is no way to hand it an already-validated transaction and
  skip `verify()`. `ValidatedPool::submit` (`graph/validated_pool.rs:293`) is `pub`, but it's
  on the internal `graph::Pool`/`ValidatedPool`, unreachable from outside without owning the
  `ChainApi`/`Pool` construction yourself.

## Two candidate approaches

**A. Pre-filter / cache-warm in `FilteringTransactionPool`** — run our own worker pool ahead
of `submit_at`, calling `LedgerApi::validate_transaction` directly to populate the soft/strict
caches, then let the stock path (`inner.submit_at`) run as normal and hit a cache.

- Pro: no changes to `sc-transaction-pool` internals; reuses the existing wrapper seam.
- **Con (ruled out below): does not avoid duplicate work.** `validate_transaction`
  (`ledger/.../mod.rs:570-590`) unconditionally deserializes the tx
  (`api.tagged_deserialize::<Transaction<S,D>>`) and fetches ledger state
  (`Self::get_ledger`) **before** checking the soft cache — the cache only skips
  `well_formed()` itself. So a "cache hit" via the stock path still re-pays: WASM entry/exit
  for the runtime-API call, storage-trie reads (`StateKey`, `MaxSkippedSlots`, weight info),
  the `TxExtension`/`CheckWeight` pipeline, the FFI crossing into the `LedgerApi` host
  function, and a second full deserialize of the tx. Proofs here run ~5KB and a tx can carry
  several, so the repeated deserialize is real CPU work, not a memcpy — likely not negligible
  next to the crypto check it's meant to save.
- Also structurally can't fully avoid the double pass anyway, since there's no way to inject
  a pre-validated tx into the stock pool without re-running `verify()` (see concurrency model
  section above).

**B. Custom `ChainApi`** — implement (or wrap) `ChainApi` ourselves so `validate_transaction`
is the *only* verification pass: batch/parallelize there, and feed the result directly into
`graph::Pool`'s own `verify()`/`submit()` flow. Delegate all other `ChainApi` methods
(`block_body`, `block_id_to_number`, `tree_route`, etc.) to `FullChainApi`-equivalent logic.

- Pro: verifies once; also removes the hardcoded 2-worker ceiling since concurrency is fully
  under our control.
- Con: more surface to build/maintain than wrapping the stock pool; diverges further from
  upstream `sc-transaction-pool`.

**Current lean: B.** The double-cost finding under A is the deciding factor — A doesn't
actually buy back the redundant deserialize/dispatch/FFI overhead it's trying to avoid.

## Separate sub-question: new runtime API vs. calling ledger host-API natively from the node

Regardless of A vs B, there's an independent question of how the (batched) verification call
itself should be implemented:

- **New runtime API** (e.g. `MidnightRuntimeApi::batch_validate_transactions`) — stays inside
  the versioned runtime boundary; a new host function backing it could still parallelize
  `well_formed()` internally (native code, so it can use a real thread pool even though the
  WASM caller is single-threaded) while paying WASM entry/exit only once per batch instead of
  once per tx.
- **Bypass the runtime, call ledger native code directly from `node`** — `node` already
  depends on `midnight_node_ledger` directly, so this is feasible. Ledger version dispatch
  (7/8/9) needs to be known; `pallets/midnight/src/lib.rs:515` (`get_ledger_version`) already
  exposes this via a (cheap, cacheable, changes only on runtime upgrade) runtime API call.
  Main risk: `pallet_midnight::validate_unsigned`'s surrounding logic — `StateKey` retrieval,
  `BlockContext`/`tblock` construction including the skipped-slots margin
  (`pallets/midnight/src/lib.rs:449-459`), and the weight pre-check
  (`check_weight`) — lives in the runtime, not the host function. Bypassing the runtime means
  reimplementing/duplicating that logic natively in the node and keeping it in sync across
  runtime upgrades. Failure mode is bounded by what this call feeds into (see below) — not yet
  decided since it depends on whether we go with A or B.

## Open questions / next steps

- Get real numbers before finalizing: `FullChainApi` already exposes
  `validate_transaction_stats` / `validate_transaction_blocking_stats`
  (Prometheus, `common/api.rs`), and the ledger side has `observe_txs_validating_time`
  (`ledger/src/versions/common/mod.rs`). Check how much of total per-tx latency is the
  `well_formed()` crypto check itself vs. surrounding dispatch/deserialize overhead — this
  bears on how much headroom option B actually buys.
- If going with B: decide `ChainApi::validate_transaction` backing — new batched runtime API
  (host function does internal parallelism) vs. direct native ledger calls from `node`
  (fastest, but duplicates block-context/state-key logic outside the runtime's forkless-upgrade
  boundary — needs a plan for keeping it in sync across runtime upgrades).
- Batching only applies to Midnight transactions specifically — need to match on
  `RuntimeCall::Midnight(MidnightCall::send_mn_transaction { .. })` the same way
  `FilteringTransactionPool::should_accept_extrinsic` already does
  (`node/src/filtering_pool.rs:119-177`), and fall through to normal per-tx handling for
  everything else.
