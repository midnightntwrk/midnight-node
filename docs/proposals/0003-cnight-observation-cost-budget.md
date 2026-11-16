# cNIGHT observation — bounding the inherent by ledger cost

Status: implemented (IDP budgeting, per-UTXO cursor, pallet fallback). Option C
below is a follow-up.

Addresses two issues: the inherent can exceed the ledger's per-block
`bytes_written` limit (→ permanent halt), and Dijkstra invalidates the `200 × 64`
count bounds. It also closes the related chain-halt and silent-skip paths in cNIGHT
observation.

## 1. The one defect behind both issues

The inherent was bounded by a **count** — `CardanoTxCapacityPerBlock (200) ×
UTXO_PER_TX_OVERESTIMATE (64)` — and the ledger bounds a block by **cost**. The
two are unrelated. The binding ledger dimension at every network's configured
limits is `bytes_written`: 224 net bytes per `Create`, 50 000 per block, so the
223rd create fits and the 224th halts the chain (`process_tokens` is Mandatory
and propagated the ledger error with `?`). Dijkstra just makes the count bound
wrong in the other direction too, and the `TooManyUtxos` rejection is itself a
halt of the same shape.

Fix the mismatch once: admit observations by **priced cost**, not by count.

## 2. Facts that shape the design (verified in this tree)

1. **Cost is content-independent.** `SystemTransaction::cost` for
   `CNightGeneratesDustUpdate` is `ccost · creates + dcost · destroys`
   (`midnight-ledger` `structure.rs`, `CNightGeneratesDustUpdate` arm). Only the
   *counts* of Create/Destroy events matter; owner, value, nonce do not.
   Registrations and deregistrations produce no ledger event, so cost 0.
2. **Pricing without applying already exists.** `LedgerApi::get_transaction_cost`
   v2 dispatches on the header tag and prices a `SystemTransaction` from the
   parameters alone (`ledger/src/ledger_9/mod.rs`, `get_system_transaction_cost`).
   It returns a scalar `GasCost` = (max normalized dimension) × `max_block.ref_time`,
   or `Err(BlockLimitExceededError)` if the tx *alone* exceeds any limit.
   `migrations/v2.rs::apply_batch` uses it exactly this way.
3. **A sound upper bound from two host calls.** Because normalization is linear
   per dimension and `max` is subadditive,
   `gas(c creates, d destroys) ≤ c · gas(1 create) + d · gas(1 destroy)`.
   Price one singleton of each type per block and the rest is integer arithmetic.
   Measured against the undeployed genesis: `create_gas` = 4 480 000 001 of
   10¹² (224/50 000 of the block, rounded up), `destroy_gas` = 3 083 454 209.
4. **The pessimistic assumption needs no storage.** Treat every `AssetCreate`
   as a would-be Create and every `AssetSpend` as a would-be Destroy. That is
   the worst case for the batch regardless of who is registered.
5. **Headroom is required.** In runtime pallet order `CNightObservation`
   precedes `C2MBridge`, so the bridge's `DistributeNight` system txs see the
   fullness cNIGHT leaves behind — and `pallet-c2m-bridge` *swallows*
   block-limit errors, so a full block silently drops bridge transfers. User
   transactions follow both. The budget must be a fraction of the limits.
6. **Rollback primitives exist.** `frame_support::storage::with_storage_layer`
   (already used in `pallets/federated-authority`), and
   `is_block_limit_exceeded` on the executor trait.
7. **Consensus visibility.** `check_inherent` byte-compares `utxos` and
   `next_cardano_position` against the importing node's own IDP output. Any
   IDP change is a coordinated binary upgrade; any pallet change is a runtime
   upgrade.

## 3. Design

**The IDP does the budgeting, pessimistically.** The pallet gets a safety
fallback only. The count bound is deleted.

### 3.1 Runtime (`pallets/cnight-observation`, `runtime/src/lib.rs`)

| Item | Change |
| --- | --- |
| `CNightObservationApi::get_observation_cost_model() -> Option<ObservationCostModel>` | new, `#[api_version(2)]`. Prices one observed UTXO by `WeightInfo::process_tokens` and a one-Create and a one-Destroy cNIGHT-generates-DUST update (zero dust key, zero value/time/nonce — fact 1); returns `{ utxo_gas, create_gas, destroy_gas, budget_gas, full_gas }`. `None` if the ledger cannot price or the weight has no per-UTXO term; the IDP then submits an empty batch at the current cursor, halting observation (never the chain) until a runtime upgrade restores pricing. It does not fall back to the count bound, which can skip rows. |
| storage `ObservationBudgetPercent: u32` | new, `ValueQuery`, default 50. `budget_gas = max_block.ref_time × percent / 100`. |
| call `set_observation_budget_percent` | new root call, `call_index` 5, `≤ 100`. |
| `TooManyUtxos` check + error | **deleted**. `#[pallet::weight]` is `T::WeightInfo::process_tokens(utxos.len())`. The count parameters leave the pallet: the never-written `CardanoTxCapacityPerBlock` storage item is removed and `get_utxo_capacity_per_block` is removed from API version 2. Nodes still call it on version 1 runtimes by its exported name, through `CallApiAt`. Under a cost model the node applies no count bound; the fetch row cap is derived from the model. |
| `process_tokens` | position checks, loop, cursor write, apply and events run inside one `with_storage_layer`. `Err(e)` with `is_block_limit_exceeded(&e)` → everything rolled back, `NextCardanoCursor` unchanged, `ObservationsDeferred` emitted, `Ok` returned. Any other error propagates as before. `InherentExecutedThisBlock` is set outside the layer. |
| storage `NextCardanoCursor: CardanoCursor` | replaces `NextCardanoPosition`. `CardanoCursor { position, utxo_offset }`: `utxo_offset` is how many UTXOs of the transaction at `position`, in the observation order (§3.5), are processed, and so the index of the next one. `apply_observations` sets it to the number of trailing batch UTXOs at `next_cardano_position`, plus the previous offset when the position did not move. Inside the storage layer, so a deferred batch leaves it. The single-block `migrations::cursor::MigrateToNextCardanoCursor` moves the old position in with offset 0; it is keyed on the old value, not the storage version, which the multi-block migrations raise after it runs. |
| `CNightObservationApi::get_next_cardano_cursor() -> CardanoCursor` | new, `#[api_version(2)]`. Replaces `get_next_cardano_position`, which nodes still call on version 1 runtimes by its exported name, through `CallApiAt`. |
| `CmstHeader` | gains `utxo_offset: u32`, the offset after the batch, so the header is the whole cursor and unique per batch. Changes the `SystemTransactionApplied` event encoding (indexer, midnight-architecture `cmst-header` spec). |
| event `ObservationsDeferred` | new. Means the native price disagrees with the ledger — a bug, fixable by runtime upgrade — and the chain stalls visibly instead of halting. |

No `STORAGE_VERSION` bump (new items have defaults). No change to `CardanoPosition`,
the `ObservedUtxo*` encodings, the inherent payload, `check_inherent`, genesis or
chainspecs, or the order observed UTXOs are sorted in (§3.5).

### 3.2 Node (`primitives/mainchain-follower`)

`get_utxos_up_to_capacity` takes the cursor, `(start_position, start_offset)`, and an
`ObservationBound`: `Count(tx_capacity)` for API v1 runtimes and genesis
construction, `Cost(model)` otherwise. It

1. fetches each UTXO kind from `start_position` with a row cap of
   `tx_capacity × 64` under `Count`, or `start_offset + full_gas / utxo_gas + 1`
   under `Cost`, noting for each kind whose raw rows filled the cap the position of
   its last row;
2. sorts in the observation order (§3.5);
3. drops the first `start_offset` UTXOs at `start_position` (already observed);
4. admits a prefix (`admit_prefix`).

```
Count(cap): per Cardano transaction: num_txs += 1; if num_txs == cap → TxCapacity
                                     (today's cap−1, byte-identical)
Cost(m):    per UTXO u:
              if u.position ≥ frontier                      → Frontier(frontier)
              bound = gas + m.utxo_gas + (m.create_gas if AssetCreate
                                          | m.destroy_gas if AssetSpend | 0)
              if bound > m.budget_gas and admitted non-empty → Cost(u.position)
              gas = bound; admit u
NotSaturated
```

`end`: `NotSaturated` → tip (as today); `TxCapacity` → `last.tx_position.increment()`
(as today); `Cost(pos)` and `Frontier(pos)` → `pos`. A `Cost` cut may fall inside a
transaction; the runtime then records the offset and the next batch resumes after it,
so a Cardano transaction of any size drains over as many blocks as it needs. The first
UTXO is always admitted, so the batch always advances.

**The fetch frontier** is the lowest last-row position among kinds that filled their
row cap. Such a kind may have further rows in that transaction or later ones, and SQL
orders rows inside a transaction differently from the observation order, so nothing
at or past the frontier is admitted; the next batch fetches it afresh. The frontier is
taken from raw rows, not decoded UTXOs: rows that do not decode (bad datums, holders
without a stake credential) cost nothing, so a cap filled by them would otherwise let
the batch reach the tip past rows never fetched. The frontier
advances unless one Cardano transaction holds more than `full_gas / utxo_gas`
(~27 000) rows of one kind, which Cardano's transaction size rules out.

The IDP reads `api.api_version::<dyn CNightObservationApi<Block>>(parent_hash)` and,
only when `≥ 2`, calls `get_observation_cost_model` and `get_next_cardano_cursor`.
A `Some` model with a positive `utxo_gas` gives `Cost`; anything else halts
observation (an empty batch at the current cursor). Version 1 runtimes give
`Count(get_utxo_capacity_per_block())`.

### 3.3 Hard requirement: the new binary against the old runtime

Between deploying the binary and the runtime upgrade, nodes on this binary sit
on a runtime without the new API, alongside nodes on the old binary. Nothing
observable may change in that window:

1. No existing runtime API method, type or call signature changes; the new
   methods are added under `#[api_version(2)]`.
2. Every new call is version-gated on the *parent* runtime, so the first block
   that can be cut by cost is the first block whose parent runs the new runtime.
   Calling a v2 method on a v1 runtime is an error, not a `None`.
3. On v1 the offset is 0 and the output is byte-identical to today's — same
   events, same `end`, same `tx_capacity − 1` — enforced by a parity test that keeps
   a verbatim copy of the shipped loop and compares the SCALE-encoded inherent.
4. Under `Count` the SQL row cap (`tx_capacity × 64`) is unchanged.

After the runtime upgrade old-binary nodes *do* diverge (they don't cut), so
all validators must be on the new binary before the upgrade; the reverse
direction is not supported.

### 3.4 Throughput

Pessimism admits ~111 creates per block at 50 % whether or not their owners are
registered — the bound cannot see registrations. Registrations and deregistrations
cost their pallet weight, roughly a thirtieth of a create at current weights, so a
flood of them is bounded at a few thousand per block rather than only by the fetch
cap. Catch-up after a long outage is therefore slower than an exact count would
allow; option C fixes that if it matters in practice.

### 3.5 The observation order is part of the API

`CardanoCursor::utxo_offset` counts UTXOs of one Cardano transaction in the observation
order, so from API v2 that order is consensus state that outlives a block. It is the
data order every runtime since `DATA_ORDERED_UTXOS_SPEC_VERSION` already sorts by,
`cmp_observed_utxos_by_data`, unchanged:

1. Cardano block number, then transaction index in the block;
2. the kind: deregistration, registration, spend, create. One Cardano output can
   yield two UTXOs (a registration and a create when it holds both the auth token
   and cNIGHT, a deregistration and a spend when such an output is spent), and the
   kind separates them;
3. the UTXO's creating transaction hash, then its output index.

db-sync yields one row per output and kind, so distinct UTXOs never compare equal.
A v2 runtime is always above `DATA_ORDERED_UTXOS_SPEC_VERSION`, so the legacy
`Ord for ObservedUtxo` never orders a batch with an offset and stays as it was for
historical blocks. Changing the order needs a new API version whose change takes
effect only once the offset is 0, so a transaction in flight drains under the order
it started with. The pallet logs, but does not reject, a batch that is not strictly
ascending: rejection would turn a node bug into a Mandatory-inherent halt, and
`check_inherent` already pins the batch across nodes.

## 4. Alternatives considered

- **A. IDP-only budgeting** (no runtime change): keeps the halt vector in the
  runtime and the count check with it. Rejected; the fallback and the deleted
  count bound are what remove the halt.
- **B. Pallet-side admission** (the pallet prices and cuts, with a storage layer
  per Cardano transaction): exact, but duplicates the budgeting in consensus and
  pays a storage layer per transaction every block for a bound the IDP already
  guarantees. Rejected in favour of budgeting once, in the IDP, with the pallet
  as a last line of defence.
- **C. `plan_observation` runtime API** (IDP asks the pallet for the exact
  admitted prefix on a throwaway overlay): zero pessimism. Follow-up if catch-up
  throughput is a problem.
- **D. Sub-transaction cursor**: adopted (§3.2, §3.5). Once thought needed only
  above Cardano's `maxTxSize`, but that counted creates (outputs); a transaction
  can spend far more cNIGHT inputs than it creates outputs: ~320 spends price above
  a whole block, and mainnet has carried a transaction with 427.
- **Levers**: raising `bytes_written` via `OverwriteParameters` (needs the
  toolkit's `--block-limit-*` flags fixed, `update_ledger_parameters.rs`) or
  lowering the count bound (cannot reach a safe value: 223 creates
  fit in 3 Cardano txs).

## 5. Tests

- Pallet: `observation_cost_model` returns `Some` with `budget = full/2`; the
  most creates the model says fit a block apply and one more is deferred with
  the cursor and the batch's registration rolled back; an inherent above the old
  count bound is accepted; setter is root-only and rejects > 100.
- Pallet: a batch ending inside a transaction sets the cursor's `utxo_offset` and the
  header's `utxo_offset`; a second batch in that transaction accumulates; a deferred
  batch leaves it; a batch past the transaction resets it to 0.
- Data source: `admit_prefix(…, Count(n))` is SCALE-byte-identical to the shipped
  loop over generated batches and capacities; the observation order is independent
  of input order and strict over distinct UTXOs; cost cuts
  may fall inside a transaction; every UTXO is priced by weight; the frontier and the
  offset skip; a 427-spend transaction drains over three blocks under runtime-shaped
  prices, every UTXO exactly once and every batch within budget.
- E2E (`tests/e2e/tests/cnight/observation.rs`): the halt reproduction
  inverted — register once, mint one more create than a block affords over
  several Cardano transactions, assert block height keeps advancing and every
  create's dust entry lands.

## 6. Rollout

1. Node binaries to all validators first. Against the old runtime they behave
   exactly as the old binary (§3.3), so mixed fleets are fine.
2. Runtime upgrade only once every validator is on the new binary. Budgeting and
   the fallback activate atomically at the upgrade block. Old binaries are not
   supported past this point.
