# BuilderContext abstraction — migration guide

Tracking issue: [midnightntwrk/midnight-node#1186](https://github.com/midnightntwrk/midnight-node/issues/1186) — let the toolkit drive transaction building from an indexer instead of replaying every block locally.

This document describes a WIP refactor on branch `oscar-builder-context-trait` that introduces a `BuilderContext<D>` trait so the toolkit's transaction builders depend on an abstract context instead of the concrete `LedgerContext`. The local backend (`LedgerContext`) is fully implemented; a future indexer-backed backend becomes a drop-in second implementation.

## What is done

- **`ledger/helpers/src/versions/common/context/builder_context.rs`** — the finalized `BuilderContext<D>` trait.
- **`ledger/helpers/src/versions/common/context.rs`** — `impl BuilderContext<D> for LedgerContext<D>` (real implementations, no stubs).
- **Reference builders (use as copy-paste templates):**
  - `utxo_output.rs` — the **sync** pattern (wallet-only builder).
  - `utxo_spend.rs` — the **async** pattern (builder that queries ledger state).

The crate does **not** compile yet: the original WIP commit left several builders half-migrated, and the files listed under "Remaining work" still use the concrete `Arc<LedgerContext>`.

## Finalized trait surface

Sync (hold a local lock across a sync closure): `with_wallet_from_seed`, `with_wallets_from_seeds`.

Async (via `#[async_trait]` — the trait is `dyn`-used heavily, so the boxed-future desugaring is centralized by the macro):
`latest_block_context`, `ledger_parameters`, `network_id`, `unshielded_utxos(seed) -> Vec<(Utxo, Timestamp)>`, `zswap_state`, `contract_state(address)`, `resolver`, `update_resolver`.

Generic: `well_formed<S, P, B>(tx, now) -> Result<()>` — the local backend checks against its `LedgerState`; an indexer backend cannot and relies on the node validating on submission.

Removed vs. the first WIP: `tx_context` (no builder uses it) and the owned `wallet_from_seed` variant.

### Indexer API mapping (validated against indexer-api `schema-v4.graphql`)

| Method | Indexer support |
|---|---|
| `ledger_parameters` | `Block.ledgerParameters` (opaque blob, deserialize client-side) |
| `network_id` | not exposed by the indexer — supply from toolkit config (already known as `received_tx.network_id`) |
| `unshielded_utxos` | `unshieldedTransactions(address)` created/spent → reconcile; `UnshieldedUtxo` carries `ctime` |
| `zswap_state` | `zswapMerkleTreeCollapsedUpdate` + `Block.zswapMerkleTreeRoot` (reconstruct) |
| `contract_state` | `contractAction(address).state` (opaque blob, deserialize) |
| `latest_block_context` | `block()` (omit offset = latest) → `Block.timestamp` |
| `well_formed` | no whole-`LedgerState` dump exists — indexer backend must skip and defer to node validation |

## Build/verify in the dev sandbox

In-sandbox `cargo` and any **populated** target dir are frozen against edits (they report stale "0 errors"). For a real check, use a **fresh** target dir — this forces a full from-scratch dependency rebuild:

```sh
CARGO_TARGET_DIR=/tmp/ck-N cargo check -p midnight-node-toolkit --message-format=short   # bump N each run
```

For iterating, rely on rust-analyzer's live diagnostics; do a fresh-target full build only at milestones.

## Migration rules

1. **Generic threading.** Anything that holds/takes `Arc<LedgerContext<D>>` (ledger/helpers) or `Arc<LedgerContext<DefaultDB>>` (toolkit) becomes generic over `C: BuilderContext<D>` / `C: BuilderContext<DefaultDB>` and uses `Arc<C>`. Drop the `LedgerContext` import; add `BuilderContext`.
2. **Trait objects.** `Box<dyn BuildInput<D>>` → `Box<dyn BuildInput<D, C>>`, and likewise for `BuildOutput`, `BuildTransient`, `BuildContractAction`, `BuildIntent`, `BuildUtxoSpend`, `BuildUtxoOutput`. Structs that hold these (`OfferInfo`, `IntentInfo`, `UnshieldedOfferInfo`, `StandardTrasactionInfo`, …) gain and propagate the `C` param.
3. **Replace every `with_ledger_state` call** with the matching async method, then `.await`:

   | Old `with_ledger_state` access | New trait call |
   |---|---|
   | `s.parameters`, `.parameters.global_ttl`, `fees_with_margin(&s.parameters, ..)` | `context.ledger_parameters().await` |
   | `ls.network_id` | `context.network_id().await` |
   | `wallet.unshielded_utxos(ls)`, iterating `ls.utxo.utxos`, reading `.ctime` | `context.unshielded_utxos(seed).await` → `Vec<(Utxo, Timestamp)>` |
   | `(*ls.zswap).clone()` | `context.zswap_state().await` |
   | `ls.index(addr)` | `context.contract_state(addr).await` |
   | `tx.well_formed(&ref_state, ..)` (the `validate` fn) | `context.well_formed(&tx, now)?` |
   | `self.context.resolver().await` (field + lock) | `context.resolver().await` |

4. **Sync vs async.** `with_wallet_from_seed` / `with_wallets_from_seeds` stay sync. Builders touching only wallets stay sync (see `utxo_output.rs`). Builders that hit any state query become async; annotate the trait and its impls with `#[async_trait]` and write the methods as plain `async fn` (template from `utxo_spend.rs`):
   ```rust
   #[async_trait]
   pub trait BuildUtxoSpend<D: DB + Clone, C: BuilderContext<D>>: Send + Sync {
       async fn build(&self, context: Arc<C>) -> UtxoSpend;
   }
   ```
   The macro expands these to `Pin<Box<dyn Future<Output = T> + Send + '_>>` under the hood, so the methods are still object-safe and `Box<dyn BuildUtxoSpend<…>>` keeps working.
5. **`.await` newly-async inherent calls.** `latest_block_context()` and `ledger_parameters()` were sync inherent methods; through the trait they return futures, so `.tblock` / `.global_ttl` accesses need `.await` first.

## Remaining work

### ledger/helpers/src/versions/common/
- `intent.rs` — `impl … for IntentInfo<D>` → `IntentInfo<D, C>`. `guo.build(ctx)` and `input.signing_key(ctx)` now hit the async `UnshieldedOfferInfo::build` (`.await`) and sync `BuildUtxoSpend::signing_key`. Fix the `IntentCustom` `BuildContractAction` impl lifetime: copy `self.resolver` (`&'static`, `Copy`) out before `Box::pin(async move …)`.
- `unshielded_offer.rs` — `UnshieldedOfferInfo<D, C>`; box fields carry `C`. `build`/`build_inputs` async; `build_outputs` sync.
- `transaction.rs` — `StandardTrasactionInfo<D, C>`, `ClaimMintInfo<D, C>`, `FromContext<D, C>`, the `validate` fn. Replace `with_ledger_state` / `.ledger_state.lock()` / `.wallets.lock()` / `.resolver()` / `Self::validate(..)` per the table; dust machinery iterates `funding_seeds` via `with_wallet_from_seed`. Add `.await`s (lines ~150, 151, 391, 432, 438).
- `contract/mod.rs` + `contract/{call,deploy,maintenance,merkle_tree}.rs` — thread `C` into `BuildContractAction<D, C>` impls and the `transcript`/`operation`/`contract_call` helpers; `ls.index(addr)` → `contract_state(addr).await`, parameters for `partition_transcripts` → `ledger_parameters().await`. Become async.
- `offer.rs`, `input.rs`, `output.rs`, `transient.rs` — mostly threaded already; verify box params carry `C` and state access uses new methods.

### util/toolkit/src/tx_generator/builder/builders/common/
Make each builder generic `<C: BuilderContext<DefaultDB>>` storing `Arc<C>`: `single_tx.rs` (partial), `batch_single_tx.rs`, `claim_rewards.rs`, `contract_call.rs`, `contract_custom.rs`, `contract_deploy.rs`, `contract_maintenance.rs`, `deregister_dust_address.rs`, `register_dust_address.rs`, `build_txs_ext.rs`, `do_nothing.rs`, `batches.rs`. Replace `with_ledger_state` reads (`contract_custom.rs:302,429`, `contract_maintenance.rs:221`, `deregister_dust_address.rs:85`, `register_dust_address.rs:57,104`). `register_dust_address.rs:162` calls `latest_block_context()` inside the **sync** helper `generationless_fee_availability` — pre-fetch the block context at the async caller and pass it in.

**`batches.rs`** is kept pinned to the concrete `Arc<LedgerContext<DefaultDB>>` rather than parameterised over `C: BuilderContext<…>`. It calls `LedgerContext::update_from_tx` between chained batches to advance the local ledger state — a write-side operation the trait deliberately does not expose (an indexer backend has no equivalent), so going generic here would require either widening the trait or restructuring the chained-prove loop. The trait-object signatures inside the file still take the new `<DefaultDB, LedgerContext<DefaultDB>>` parameters; only the storage of `context` stays concrete.

### util/toolkit/src/tx_generator/builder/mod.rs
`to_builder_v8` / `to_builder_v7` construct builders with the concrete `Arc<LedgerContext<DefaultDB>>` (ledger_8 / ledger_7), monomorphizing `C = LedgerContext<DefaultDB>`. The generic bottoms out here; `BuildTxs` itself needs no `C` param (context lives in the builder). A turbofish on `::new` may be needed for inference.

### New file: context/indexer_context.rs
`pub struct IndexerContext<D>(PhantomData<D>);` + `impl<D: DB + Clone> BuilderContext<D> for IndexerContext<D>` with every method `todo!("indexer: …")`. Declare `pub mod indexer_context;` in `context.rs`. Purpose: prove the trait is implementable by a non-`LedgerContext`; `network_id` and `well_formed` are the methods the indexer genuinely can't serve. Do not wire it into any binary.

## Verification
1. Fresh-target `cargo check -p midnight-node-toolkit` (bump the dir name).
2. `cargo test -p midnight-node-ledger-helpers offer::tests`.
3. `rg 'Arc<LedgerContext<' util/toolkit/src/tx_generator ledger/helpers/src/versions/common` — only test code and the `mod.rs` dispatch construction should remain.
4. Add `changes/changed/builder-context-trait.md`; squash the WIP commits into a clean `refactor:` before opening a PR.
