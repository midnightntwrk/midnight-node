# cNIGHT observation — bounding the inherent by ledger cost

Status: implemented (IDP budgeting + pallet fallback). Options C and D below
are follow-ups.

Addresses
[SSE-599](https://github.com/shieldedtech/shielded-security-engineering/issues/599)
(inherent can exceed the ledger's per-block `bytes_written` limit → permanent halt) and
[SSE-590](https://github.com/shieldedtech/shielded-security-engineering/issues/590)
(Dijkstra invalidates the `200 × 64` count bounds). Companion to
`0002-sub-transaction-cardano-position.md`, which this document settles the
"why" for.

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
| `CNightObservationApi::get_observation_cost_model() -> Option<ObservationCostModel>` | new, `#[api_version(2)]`. Prices one observed UTXO by `WeightInfo::process_tokens` and a one-Create and a one-Destroy cNIGHT-generates-DUST update (zero dust key, zero value/time/nonce — fact 1); returns `{ utxo_gas, create_gas, destroy_gas, budget_gas, full_gas }`. `None` if the ledger cannot price or the weight has no per-UTXO term; the IDP then submits an empty batch at the current `NextCardanoPosition`, halting observation (never the chain) until a runtime upgrade restores pricing. It does not fall back to the count bound, which can skip rows. |
| storage `ObservationBudgetPercent: u32` | new, `ValueQuery`, default 50. `budget_gas = max_block.ref_time × percent / 100`. |
| call `set_observation_budget_percent` | new root call, `call_index` 5, `≤ 100`. |
| `TooManyUtxos` check + error | **deleted**. `#[pallet::weight]` is `T::WeightInfo::process_tokens(utxos.len())`. The count parameters leave the pallet: the never-written `CardanoTxCapacityPerBlock` storage item is removed and `get_utxo_capacity_per_block` is removed from API version 2. Nodes still call it on version 1 runtimes by its exported name, through `CallApiAt`. Under a cost model the node applies no count bound; the fetch row cap is derived from the model. |
| `process_tokens` | position checks, loop, cursor write, apply and events run inside one `with_storage_layer`. `Err(e)` with `is_block_limit_exceeded(&e)` → everything rolled back, `NextCardanoPosition` unchanged, `ObservationsDeferred` emitted, `Ok` returned. Any other error propagates as before. `InherentExecutedThisBlock` is set outside the layer. |
| event `ObservationsDeferred` | new. Means the native price disagrees with the ledger — a bug, fixable by runtime upgrade — and the chain stalls visibly instead of halting. |

No `STORAGE_VERSION` bump (new items have defaults). No change to `CardanoPosition`,
`ObservedUtxo*`, the inherent payload, `check_inherent`, `CmstHeader`, genesis or
chainspecs.

### 3.2 Node (`primitives/mainchain-follower`)

`get_utxos_up_to_capacity` takes an `ObservationBound`: `Count(tx_capacity)` for API
v1 runtimes and genesis construction, `Cost(model)` otherwise. The fetch row cap per
UTXO kind is `tx_capacity × 64` under `Count` and `full_gas / utxo_gas + 1` under
`Cost`: no UTXO costs less than `utxo_gas`, so more rows than the whole block affords
can never be admitted and the cap never truncates ahead of the cost cut. The
truncation loop is `admit_prefix`, iterating Cardano transactions (`chunk_by` on
equal `tx_position`):

```
for group in groups:
    Count(cap): num_txs += 1; if num_txs == cap → Cut::TxCapacity          // today's cap−1
    Cost(m):    g = Σ over group of m.utxo_gas + (m.create_gas if AssetCreate
                                                 | m.destroy_gas if AssetSpend | 0)
                bound = gas + g                                              // saturating
                if bound > m.budget_gas and (admitted non-empty or bound > m.full_gas)
                    → Cut::Cost(group.position)
                gas = bound
    admit group
Cut::NotSaturated
```

`end`: `NotSaturated` → tip (as today); `TxCapacity` → `last.tx_position.increment()`
(as today); `Cost(pos)` → `pos`, so the refused transaction is re-delivered whole
next block. A lone transaction between budget and full is admitted alone. A lone
transaction over `full_gas` yields an empty batch with `end = pos` and an error
log — a stall, impossible at today's `maxTxSize` (≤ ~157 outputs/tx vs 223
creates), fixed properly by option D.

The IDP reads `api.api_version::<dyn CNightObservationApi<Block>>(parent_hash)` and
calls `get_observation_cost_model` only when `≥ 2`. A `Some` model with a positive
`utxo_gas` gives `Cost`; anything else halts observation (an empty batch at the
current position). A version 1 runtime gives `Count(get_utxo_capacity_per_block())`.

### 3.3 Hard requirement: the new binary against the old runtime

Between deploying the binary and the runtime upgrade, nodes on this binary sit
on a runtime without the new API, alongside nodes on the old binary. Nothing
observable may change in that window:

1. No existing runtime API method, type or call signature changes; the new
   method is added under `#[api_version(2)]`.
2. Every new call is version-gated on the *parent* runtime, so the first block
   that can be cut by cost is the first block whose parent runs the new runtime.
   Calling a v2 method on a v1 runtime is an error, not a `None`.
3. `cost_model == None` is byte-identical to today's output — same events, same
   `end`, same `tx_capacity − 1` — enforced by a parity test that keeps a
   verbatim copy of the shipped loop and compares the SCALE-encoded inherent.
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
allow; option C fixes that if it matters in practice. Transaction granularity wastes
at most one transaction's cost per block.

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
- **D. Sub-transaction cursor** (`0002`): only needed when a *single* Cardano
  transaction exceeds the whole block. Follow-up on trigger: Cardano `maxTxSize`
  above ~24 KiB or a lower `bytes_written`.
- **Levers**: raising `bytes_written` via `OverwriteParameters` (needs the
  toolkit's `--block-limit-*` flags fixed, `update_ledger_parameters.rs`) or
  lowering the count bound (cannot reach a safe value: 223 creates
  fit in 3 Cardano txs).

## 5. Tests

- Pallet: `observation_cost_model` returns `Some` with `budget = full/2`; the
  most creates the model says fit a block apply and one more is deferred with
  the cursor and the batch's registration rolled back; an inherent above the old
  count bound is accepted; setter is root-only and rejects > 100.
- Data source: `admit_prefix(…, Count(n))` is SCALE-byte-identical to the shipped
  loop over generated batches and capacities; cost cuts fall on transaction
  boundaries with `end` at the refused position; every UTXO is priced by weight and
  registrations alone are bounded; lone-transaction rules; the fetch row cap never
  truncates ahead of the cost cut.
- E2E (`tests/e2e/tests/cnight/observation.rs`): SSE-599's `churn_halt`
  inverted — register once, mint one more create than a block affords over
  several Cardano transactions, assert block height keeps advancing and every
  create's dust entry lands.

## 6. Rollout

1. Node binaries to all validators first. Against the old runtime they behave
   exactly as the old binary (§3.3), so mixed fleets are fine.
2. Runtime upgrade only once every validator is on the new binary. Budgeting and
   the fallback activate atomically at the upgrade block. Old binaries are not
   supported past this point.
