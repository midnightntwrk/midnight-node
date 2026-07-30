# Batch Proof Verification — Investigation Notes

# Mempool (Block Production)

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

## Batch processing

### Algorithm for batching

Suggested batching algorithm:

```
params:
  M         # hard max batch size (hardware limit)
  k_target  # efficient batch size, ~the knee of the cost curve, <= M
  tau       # max age of the oldest queued item before forced dispatch

# each of the N workers, when free:
loop:
    wait until queue non-empty
    while queue.size < k_target
          and queue.oldest.age < tau:
        wait (on condition var, with timeout = tau - queue.oldest.age)
    batch = queue.take(min(queue.size, M))   # grab as much as is efficient
    process(batch)                            # T = alpha + beta*|batch|
```

In the case of one bad transaction in the batch, the whole batch fails together. The fallback mechanism needs work - for now, fallback should be verifying each tx individually

### Implementation

<TODO>

# Block import

Goal (mirrors the mempool investigation, but for the receive/import side): today,
`well_formed()` runs once per Midnight extrinsic, serially, as the block executor walks
the block in order. Can we batch that proof check across the whole block instead?

## Node-side import pipeline: who actually calls `execute_block`

The "current validation flow" below starts at the runtime's `Core::execute_block`, but that's
a *runtime API* — something on the **node** side has to call it. Tracing that back (this is the
import-side analogue of the `service.rs` → pool wiring documented in the mempool section):

1. **`node/src/service.rs:428`** — `partner_chains_aura_import_queue::import_queue(ImportQueueParams
   { block_import: grandpa_block_import.clone(), .. })` builds a stock `sc_consensus::BasicQueue`.
   Two things are wired in: a **verifier** (partner-chains `AuraVerifier`) and a **block-import**
   (`grandpa_block_import`, `service.rs:437`).
   - Note: BEEFY's block-import is constructed at `service.rs:411` but its import half is dropped
     (`let (_, beefy_voter_links, ..)`) — only its links are kept. So the import chain is
     queue → GRANDPA → client; **BEEFY is not in it**.
2. **Queue worker** (`sc_consensus::import_queue` / `BasicQueue`) — for each queued block runs
   `Verifier::verify`, then hands the block to `BlockImport::import_block`.
3. **`partner-chains/substrate-extensions/aura/consensus/src/import_queue.rs:158`**
   (`AuraVerifier::verify`) — checks the seal/slot (`check_header`) and the **inherents**
   (`check_inherents_with_data` → `BlockBuilder_check_inherents` runtime API =
   `data.check_extrinsics(&block)`, `runtime/src/lib.rs:1369`). It does **not** run
   `Executive::execute_block`, so **`well_formed()` does not run in the verifier** — the inherent
   check only validates inherent data (timestamp, etc.), not the signed/unsigned user txs. It also
   early-returns entirely for state-only / gap sync (`import_queue.rs:167`).
4. **`grandpa/.../import.rs:538`** (`GrandpaBlockImport::import_block`) — does GRANDPA
   authority-set bookkeeping, then delegates to `(&*self.inner).import_block(block)`. `inner` is
   the `Client` (`sc_consensus_grandpa::block_import(client.clone(), ..)`, `service.rs:403`).
5. **`sc_service::client::Client::import_block`** (`.../substrate/client/service/src/client/client.rs:1852`
   → `:1742`) → `prepare_block_storage_changes` (`:809`).
6. **`client.rs:860`** — `runtime_api.execute_block(parent_hash, Block::new(header, body))`.
   **This is the node code that calls the `Core_execute_block` runtime API.** Afterwards it reads
   the resulting storage changes and rejects the block with `InvalidStateRoot` if the computed root
   ≠ `header.state_root()` (`:870`).

Full node→runtime chain: BasicQueue worker → `AuraVerifier::verify` (seal + inherents, **no
execution**) → `GrandpaBlockImport::import_block` → `Client::import_block` →
`prepare_block_storage_changes` → **`client.rs:860 runtime_api.execute_block`** → runtime
`Core::execute_block` (`runtime/src/lib.rs:1274`, step 1 of the next section).

**Important — only blocks received from peers hit `execute_block` on import.**
`prepare_block_storage_changes` re-executes *only* when state must be enacted **and** no storage
changes were supplied with the block (`client.rs:841-847`, the `(true, None, Some(body))` arm). A
block **this** node authored is imported with its storage changes already attached
(`StateAction::ApplyChanges(StorageChanges::Changes(..))`, computed during authoring), so it takes
the `(true, Some(changes), _)` arm at `client.rs:844` and **skips `execute_block` entirely** — its
proofs were already checked during authoring / from the pool. Import-side batching therefore only
speeds up **received** blocks (initial sync + gossiped peer blocks), which is exactly the case that
matters; it's complementary to the mempool-side batching, which covers the blocks we author.

**This yields a clean node-side seam — the import-side analogue of `FilteringTransactionPool`.**
A custom `BlockImport` wrapper inserted in `service.rs` (wrapping `grandpa_block_import`, or sitting
between the queue and it) can run the batched `well_formed()` pre-verification in its own
`import_block`, against the parent state (`parent_hash` is on `BlockImportParams`), **before**
delegating to the inner import that eventually reaches `client.rs:860`. Because
`BlockImport::import_block` returns `Result<ImportResult, _>`, a bad block is rejected **gracefully —
no panic**. That resolves the placement question below: the fail-fast pre-scan does **not** have to
live inside `Core::execute_block` (the panic channel). It's the same "separate call before
`execute_block`" option floated under "Signaling a bad block," but sited in the node import pipeline
as a `BlockImport` wrapper rather than as a new runtime API the node invokes by hand.

## Current validation flow during import (top to bottom)

1. **`runtime/src/lib.rs:1274`** — `impl sp_api::Core<Block> for Runtime::execute_block`
   receives `<Block as BlockT>::LazyBlock` and calls `Executive::execute_block(block)`.
   `Executive = frame_executive::Executive<...>` (`runtime/src/lib.rs:1110`) is an external,
   generic crate (`~/.cargo/git/checkouts/polkadot-sdk-.../substrate/frame/executive`) — not
   something to fork/modify, and it has no per-pallet batching seam.
2. **`frame/executive/src/lib.rs:695-728`** (`Executive::execute_block`) — pulls
   `block.extrinsics()` and calls `apply_extrinsics` (`:750`), which iterates extrinsics
   **one at a time**, in block order, calling `do_apply_extrinsic` per item.
3. **`frame/executive/src/lib.rs:862-920`** (`do_apply_extrinsic`) — decodes the extrinsic,
   then `Applyable::apply::<UnsignedValidator>` (`:898`), which for unsigned extrinsics runs
   `ValidateUnsigned::pre_dispatch` **before** `dispatch`.
4. **`pallets/midnight/src/lib.rs:464`** (`Pallet::pre_dispatch`) → `validate_guaranteed_execution`
   → **`ledger/src/versions/common/mod.rs:959`** (`get_verified_transaction` →
   `tx.0.well_formed(...)`) — the actual ZK proof check. This is the serial bottleneck: one
   `well_formed()` call per Midnight extrinsic, run strictly in block order, one at a time.
5. `dispatch` then calls `send_mn_transaction` (`pallets/midnight/src/lib.rs:373`) →
   `apply_transaction` (`ledger/.../mod.rs:314`), which calls `get_verified_transaction` again
   with the same cache key — a guaranteed `STRICT_TX_VALIDATION_CACHE` hit (populated by step
   4), so `well_formed()` doesn't run twice per tx. The real cost is entirely in step 4.

## Is batching against pre-block state actually correct?

Checked `well_formed()`'s real implementation (`midnight-ledger` 7.0.3, `src/verify.rs:560-565`).
Its own doc comment: "performs all checks possible with **a moderately stale reference state**."
It's structured around a `StateReference` trait (`verify.rs:63`) with named check categories:

- `stateless_check` — proof/signature/binding-commitment verification (the expensive crypto).
  Doesn't read state at all.
- `param_check` / `op_check` / `maintenance_check` — ledger parameters, a contract's registered
  operation/verifier key, maintenance authority. All **code-level** state (changes only on
  contract deploy/maintain), not per-call data state.
- Notably **absent**: any nullifier/UTXO-double-spend check. That's enforced later, at
  `Ledger::apply_verified_transaction` (actual state mutation) — sequential, unaffected by
  batching either way.

Conclusion: batch-verifying a whole block's proofs against the state snapshot as of the
**start of the block** (before any of that block's txs are applied) is the same kind of
staleness the design already tolerates today — the mempool path already validates against
state that's staler still. Not a new correctness hazard, just a bit more of an
already-accepted one.

## The real wrinkle: cache key granularity

`STRICT_TX_VALIDATION_CACHE` is keyed by `state_hash + tx_hash` (`ledger/.../mod.rs:944`), and
`state_hash` changes after every applied extrinsic (`StateKey::<T>::put(new_state_key)`,
`pallets/midnight/src/lib.rs:387`). So "batch-verify against the pre-block snapshot, then warm
the existing cache" only hits for the *first* Midnight extrinsic in the block — by the time
`pre_dispatch` runs for the second one, `StateKey` storage has already moved past the pre-block
snapshot the batch was keyed against, and it's a cache miss again.

Needs either:
- a second cache tier keyed on `tx_hash` alone (justified by the staleness-tolerance above), or
- bypassing the cache entirely for batch-verified txs and threading the "already verified"
  result into `pre_dispatch` some other way (e.g. a per-block in-memory set of tx hashes
  known-good from the batch pass).

Not yet decided which.

## What `LazyBlock` gives you for a pre-scan

`LazyBlock<Header, Extrinsic>` (`primitives/runtime/src/generic/block.rs:83`):
```rust
pub struct LazyBlock<Header, Extrinsic> {
    pub header: Header,               // fully decoded, eagerly
    pub extrinsics: Vec<OpaqueExtrinsic>,
    _phantom: PhantomData<Extrinsic>,
}
```
`OpaqueExtrinsic` (`primitives/runtime/src/lib.rs:990`) is just `bytes::Bytes` — raw,
boundary-split bytes per extrinsic; contents (signature/call/args) untouched. The
`LazyBlock::extrinsics()` trait method (`block.rs:141-143`) decodes each `Self::Extrinsic`
lazily, on pull, and takes `&self` — so a pre-scan can iterate it without consuming `block`,
then still hand `block` to `Executive::execute_block(block)` afterward.

Practical seam: inside `Core::execute_block` (`runtime/src/lib.rs:1274`), before calling
`Executive::execute_block(block)`, iterate `block.extrinsics()`, match
`RuntimeCall::Midnight(MidnightCall::send_mn_transaction { midnight_tx })`, collect the
`midnight_tx: Vec<u8>` payloads, batch-verify via one host-function call, then proceed as
normal. Cost: every extrinsic gets fully SCALE-decoded twice (pre-scan + the real loop inside
`apply_extrinsics`) — cheap relative to the proof check being batched, not a concern.

## No-std constraint on the batch call shape

`midnight-ledger` isn't no-std/WASM-compatible, so `midnight_tx` can only ever be opaque bytes
on the runtime side — `Transaction<S,D>` is only ever deserialized inside a host function,
natively. Consequence for the batch design:

- The batch host call necessarily takes `Vec<Vec<u8>>` (raw tx bytes) in, and can only return
  pass/fail (+ populate whatever cache/handoff the runtime checks later) — never a deserialized
  object handed back to the runtime.
- This does **not** eliminate the deserialize inside `apply_transaction` at dispatch time
  (`ledger/.../mod.rs:340`) — `Ledger::apply_verified_transaction` needs the fully parsed tx to
  compute state effects, not just a verified marker. That deserialize happens once per tx no
  matter what.
- So the achievable win from import-side batching is specifically "skip the redundant
  `well_formed()` pairing check," not "skip a deserialize." Same shape as the mempool-side
  option-A finding (cache hit still re-pays the deserialize) — just on the import path instead
  of the pool path.

## Signaling a bad block, and the fail-fast decision

`Core::execute_block` (`runtime/src/lib.rs:1274`) returns `()` — there is no `Result` to reject
an invalid block with. In Substrate, **import validity is signaled by panicking inside the
runtime**, not by a return value. The current serial path already relies on this: a bad proof
makes `pre_dispatch` return `TransactionValidityError` (`pallets/midnight/src/lib.rs:464`),
which propagates via `?` at `frame/executive/src/lib.rs:898` (`do_apply_extrinsic`), becomes
`Err(ExecutiveError::ApplyExtrinsic(..))` in `apply_extrinsics` (`:779`), and hits
`panic!("{:?}", e)` in `Executive::execute_block` (`:711`). The WASM executor traps that panic
on the native side and surfaces it as an import/execution error — the block is rejected and the
node does **not** crash. So "`execute_block` can't return an error" is not a blocker: panic
*is* the error channel on the import path.

Two ways the batch pass could use this:

- **Warm-only (rejected).** Batch pass only marks good txs as verified; a bad tx falls through
  to the serial `pre_dispatch`, which re-runs `well_formed()`, fails, and panics as today. Adds
  no new error surface — *but* a malicious block producer who includes a single bad-proof tx
  defeats the batch: a batch that fails as a unit yields no per-tx info, forcing a fallback to
  individual verification of the whole block. That's the full serial cost **plus** the wasted
  batch attempt — the attacker dictates a worst-case import time on demand.
- **Fail-fast (chosen).** Batch-verify the whole block's proofs up front, before applying any
  extrinsic. If the batch fails, reject the block immediately. If it succeeds, mark all txs
  known-good and proceed; every Midnight `pre_dispatch` then hits the handoff.

**Decision: fail-fast.** The key asymmetry vs. the mempool path: on *import* we never need to
know *which* tx is bad — the whole block is rejected regardless — so we can skip the per-tx
individual-verification fallback entirely. Cost of an adversarial block is then bounded by one
batched verification pass (an aggregate check, or a bounded parallel fan-out), which is no worse
than — and generally better than — today's serial one-by-one worst case (which already verifies
up to N−1 good proofs before hitting a bad one placed last). The individual-verification
fallback from the batching-algorithm section belongs to the **mempool** path, where per-tx
attribution is needed to accept the good txs and reject/penalize the bad one; it does not apply
here.

Placement determines whether the panic is even needed:
- **Inside `Core::execute_block`** — must use the panic channel (no `Result`); panic before
  `Executive::execute_block(block)` so nothing is applied first.
- **A separate call in the node-side import pipeline** *before* `execute_block` — returns a
  `Result`, so the node rejects the block gracefully without a panic and without entering
  `execute_block` at all. Concretely this is a custom `BlockImport` wrapper in `service.rs`
  (see "Node-side import pipeline" above); it can back its check with a new `pre_verify_block`
  runtime API or a direct native ledger host call. This is the same placement question flagged in
  the open questions below.

## Open questions / next steps

- Decide the cache-key-granularity question above (new `tx_hash`-only tier vs. bypass +
  handoff).
- Design the batched host function itself: same `LedgerApi` version-dispatch pattern (7/8/9) as
  the existing per-tx calls; needs to accept a `Vec<Vec<u8>>` and the pre-block `state_key`,
  return per-tx verified/invalid.
- Decide whether the fail-fast pre-scan lives directly in `Core::execute_block`
  (`runtime/src/lib.rs:1274`, panic to reject) or in a **node-side `BlockImport` wrapper**
  (wrapping `grandpa_block_import` in `service.rs`; see "Node-side import pipeline") that runs
  ahead of the `client.rs:860` `execute_block` call. The wrapper returns `Result`, so it rejects
  gracefully; it backs onto either a new `pre_verify_block` runtime API or a direct native ledger
  host call. Adds a second WASM entry/exit (if runtime-API-backed) and a second SCALE-decode pass —
  needs the same real-numbers check flagged in the mempool section. Note the wrapper only ever sees
  **received** blocks needing execution (authored blocks import with storage changes already
  attached and skip `execute_block`), so it never double-verifies our own blocks.


# Attackers sending bad txes

Is there some mechanism for penalizing bad actors if they try to spam the node?


# Ledger dependency wiring (batch-verification branch)

This branch consumes the ledger repo's `js/batch-verification` branch. How that works:

**Model.** Our workspace depends on ledger crates by their published *names/versions*
(`[workspace.dependencies]`), and `[patch.crates-io]` redirects each name to a git ref.
Three ledger versions coexist (L7/L8/L9); a patch is keyed by crate name and only applies
to the version that matches, so L7/L8 keep resolving from crates.io while L9 comes from git.

**Why the branch can't be patched in raw.** On the branch, every inter-crate dep still
carries `path = "../foo"` (e.g. `storage = { version = "2.0.0", path = "../storage", .. }`).
A git patch at the raw branch would drag `midnight-storage`/`-static` in *from the branch*,
colliding with the crates.io copies L8 uses → a source-unification error. The published rc
*tags* strip those `path=` attributes (deps become version-only), which is what lets each
crate be redirected independently. The ledger repo does this with `scripts/isolate.py`
(filters workspace members to one crate, strips `path=` from its `[dependencies]`, drops
dev-deps, commits, tags, pushes, reverts).

**What's pinned here.** The branch diverged from `crate-ledger-9.1.0.0-rc.3` (not a
descendant). Only the crates whose *own* source changed vs. that rc source (`<tag>^`) need
re-isolating; unchanged crates keep their published rc tags (they reference changed siblings
by version, which our patches redirect). Changed set:

All 7 are pinned to a **single** isolate commit (`rev = c1da0f6d…`, the tip of the ledger branch
`js/batch-verification-isolated`) — they're all workspace members of that one commit with their
`path=` deps stripped, so every patch resolves its package as a member of the same commit;
inter-crate deps among the 7 route registry→patch→same rev. (The ledger repo's own
`scripts/isolate.py` produces *one crate per tag* for crates.io publishing; that per-crate split
isn't needed for our patching — one commit suffices.)

Branch name note: it's `js/batch-verification-isolated` (hyphen), **not**
`js/batch-verification/isolated` — git can't have both a ref named `js/batch-verification` and
refs under `js/batch-verification/` (a ref can't be both a file and a directory).

| crate | dir | reason it's isolated |
|---|---|---|
| midnight-ledger-v9        | ledger/           | changed (verify/structure/dust) |
| midnight-zswap            | zswap/            | changed (verify.rs) |
| midnight-transient-crypto | transient-crypto/ | changed (proofs/mock_verify) |
| midnight-onchain-state    | onchain-state/    | changed (state.rs) |
| midnight-zkir             | zkir/             | changed (ir.rs) |
| midnight-zkir-v3          | zkir-v3/          | newly required: ledger `test-utilities` now pulls `dep:zkir_v3`, and `ledger/helpers` enables that feature |
| midnight-ledger-static    | static/           | `static/version` moved 9→10 (new Dust ZKIR-v3 artifacts) with no package-version bump, so the crates.io 9.0.0 copy would bake a stale `version!()` into the data-provider key paths |

Unchanged, on published rc tags: coin-structure, base-crypto, base-crypto-derive,
onchain-vm, onchain-runtime, serialize (+ transient-crypto-old 2.2.0, untouched).
`base-crypto`, `onchain-runtime`, `storage` and `storage-core` *did* change on the branch, but
only cosmetically (clippy `needless_borrow` fixes, a `#[allow]`, a semver-compatible `reqwest`
bump) — no API movement, so they stay on their rc tags / crates.io copies. `storage`/`storage-core`
additionally *must not* be patched: L8 resolves them from crates.io and a git patch would collide.
Side effect: `transient-crypto`/`zkir`/`zkir-v3` bumped `midnight-circuits ^7.2.1→^7.2.2`
and `midnight-zk-stdlib ^2.3.1→^2.3.3`, and `base-crypto` bumped `reqwest ^0.13.0→^0.13.4`.

**The midnight-zk stack also has to be patched here.** The ledger branch redirects
`midnight-proofs`/`-curves`/`-circuits`/`-zk-stdlib` to the `midnight-zk` branch
`irakoton/batch-verify` (which is where `batch_verify(.., identify_failures)` and the
failing-index reporting live) via its *own* `[patch.crates-io]`. A dependency's patch table is
ignored — only the root workspace's counts — so the node repeats those four redirects, pinned by
`rev` to `ae7b9aeb…`, the commit the ledger branch's own lockfile resolved. Only the `^7.2.2` /
`^0.8.1` / `^0.3.1` / `^2.3.3` requirements match the patch; `zkir`'s `-v1` aliases
(`midnight-circuits ^6.2.1` etc.) keep resolving from crates.io. Two of the midnight-zk branch's
deps (`blake2b_halo2`, `sha3-circuit`) are themselves git deps — fine, since they hang off a git
dependency rather than a patch.

**Current state.** The single isolate commit `c1da0f6d` (ledger branch tip `71fc0084`) is pushed
to the ledger remote branch `js/batch-verification-isolated`, and the node `[patch.crates-io]`
pins the 7 crates by `rev` to that commit over
`https://github.com/midnightntwrk/midnight-ledger`. No tags. This is shareable/CI-ready — no
`file://`, no machine-local dependency. The commit stays reachable as long as the
`js/batch-verification-isolated` branch exists on the remote (don't delete it).

**Dust proving keys: use the bundled artifacts, not the published ones.** The branch's
`ledger/static/dust/spend.*.sha256` expect `d058526b…`/`6ecb69fc…`/`41264553…`, but
`https://srs.midnight.network/dust/10/spend.*` serves `b9e39102…`/`3e8d4fbd…`/`d2ad8f21…`, and
there is **no plan to publish** the new ZKIR-v3 Dust artifacts for this branch. So anything that
*fetches* Dust keys through `MidnightDataProvider`/`DUST_EXPECTED_FILES` (i.e. `ledger/helpers`, so
proving-side tests and the toolkit) will fail the hash check; those paths need to source the keys
from the crate's bundled `ledger/static/dust/*` instead of the data provider.

Node-side *verification* is unaffected either way: `SPEND_VK` comes from
`include_bytes!("../static/dust/spend.verifier")`. And `zswap/10/*` is byte-identical to
`zswap/9/*`, so the `static/version` bump is a no-op for L7/L8/L9 zswap keys.

**To regenerate after the ledger branch moves** — the `js/batch-verification-isolated` branch is
kept **append-only** (each isolate commit is parented on the previous one, so pushes fast-forward;
no force-push). Steps:

1. `git fetch origin js/batch-verification js/batch-verification-isolated`
2. In a throwaway detached worktree at the new branch tip, run `consolidate-ledger-isolate.py`
   (repo root, next to this file) — it filters root members/default-members to the target dirs,
   strips `path=` from their `[dependencies]`, drops `[dev-dependencies]`, and makes one commit
   `ISO_NEW` (parented on the *new branch tip*). Full driver commands are in the script header.
3. Re-parent that snapshot onto the current `-isolated` tip so it appends:
   `APPEND=$(git commit-tree "$(git rev-parse ISO_NEW^{tree})" -p origin/js/batch-verification-isolated -m "isolate <newsha>")`
4. Fast-forward push (no force): `git push origin "$APPEND:refs/heads/js/batch-verification-isolated"`
5. Update the `rev` for the 7 crates in the node `Cargo.toml` to `$APPEND`, and re-check the ledger
   branch's root `Cargo.toml` `[patch.crates-io]` + `Cargo.lock` in case the midnight-zk pin moved.
   **Also grep for a second pin**: `ledger/helpers/Cargo.toml` has its own *direct* (non-patch)
   `zkir-v3 = { package = "midnight-zkir-v3", git = ..., rev = "…", features = ["binary"] }` for the
   `zkir compile-many` CLI (see "Dust proving keys"). It doesn't route through
   `[patch.crates-io]`, so a `grep -rn cd54fd2f… --include=Cargo.toml` (the *old* rev, not just the
   workspace root) is the reliable way to catch every pin site. Leaving it stale doesn't break the
   build — cargo just resolves *two* `midnight-zkir-v3` package instances, one per rev — but it
   silently reintroduces stale-key risk and triggers the churn in step 6.
6. Re-resolve the lock with `cargo check --offline -p midnight-ledger-v9` (or any `cargo
   build`/`check` — it doesn't need to target that specific package). A plain check/build already
   re-locks exactly the packages whose manifest `source` changed; it does **not** need `cargo update`
   at all for a pure `rev` bump. `--offline` matters: `cargo update -p <pkg>` (even scoped, even with
   `--precise <rev>`) refreshes the crates.io index and re-solves the whole graph, which can
   *downgrade* unrelated transitive picks to whatever else is now compatible — last two refreshes hit
   `bip39`'s `rand_core 0.6.4 → 0.4.2` (breaks `pallas-wallet`'s rand-0.8
   `Mnemonic::generate_in_with`, surfacing as an unrelated-looking build failure in
   `midnight-beefy-relay`) plus unrelated `windows-sys`/`socket2`/`itertools`/`syn` churn. Confirm
   with `git diff Cargo.lock`: it should touch **only** the 7 crates' `source = git+…?rev=…#…` lines
   (14 lines: old+new per crate) — a `cargo check --offline` on the previous, already-consistent pin
   left exactly that on a re-run before the second-pin fix above, and a wider diff means either the
   second pin above is still stale (temporarily produces two `midnight-zkir-v3` entries) or something
   reached the network and re-resolved more broadly than intended.

The commit is **not reproducible by SHA** (commit timestamps differ per run), so the `rev` changes
every regeneration. (The ledger repo's `scripts/isolate.py` is the per-crate/push-tags alternative
used for real crates.io releases.)

## New on the branch, not yet used here: localised batch failures

`ledger/src/structure.rs` now calls `VerifierKey::batch_verify_with_failures(.., identify_failures)`
and, on rejection, returns `MalformedTransaction::InvalidProofBatch { failed_indices }` — the
positions of the bad proofs within the transaction's `collect_proof_evidence()` sequence.

Our mempool path (`isolate_on_failure = true`) still isolates the offender by re-verifying every
ready transaction as a batch-of-one (`isolate_fallback_results`). Because we concatenate evidence
per transaction in input order, the reported indices could be mapped straight back to the offending
transactions instead, turning an O(n) re-verification into a single aggregate call. Worth doing, but
it needs `batch_verify_proofs` to return the indices rather than `Err(())`, plus a per-tx
evidence-length table — deliberately left out of the dependency bump.

Jegor also added `Intent::collect_dust_proof_evidence` (dust-only evidence collection). Purely
additive; we don't need it for the current batch shape.

## Fallout of the midnight-zk bump: static fixture txs need regenerating

`cargo test -p midnight-node-ledger --lib` (run with `-p midnight-node-e2e` too, so
`midnight-node-ledger-helpers/can-panic` is unified on — on its own the crate doesn't compile its
own lib tests) goes 99/99 → 96/99 across the bump. The three failures are all
`InvalidProof(Invalid proof)` on the committed fixture transactions:

- `ledger_9::common::api::ledger::tests::should_apply_transaction`
- `ledger_9::common::api::ledger::tests::should_get_contract_state`
- `ledger_9::common::api::transaction::tests::should_validate_transaction`

They deserialize `res/test-contract/contract_tx_{1..4}_*_undeployed.mn` and run the real
`well_formed`. The `irakoton/batch-verify` midnight-zk branch changes the proof system itself
(`nb_arith_cols` on `ZkStdLibArch`, plus `sha3-circuit`/`blake2b_halo2` moving to git revs), so
proofs produced under the old architecture no longer verify. The ledger repo hit the same thing and
had to recompute its own precompile verifier hashes (`21234ede` micro-dao, `2509be25`
simple-merkle-tree).

Fix is to regenerate the fixtures, not to change pins: `earthly -P +rebuild-genesis-state-undeployed`
(Earthfile ~line 300 builds `res/test-contract/contract_tx_*_undeployed.mn` with the toolkit right
after genesis). If that regeneration needs Dust proving keys it will hit the artifact gap above, and
has to take them from the crate's bundled `ledger/static/dust/*` rather than `srs.midnight.network`.
