# Ledger events

The node surfaces the per-transaction event stream that the [midnight-ledger](https://github.com/midnightntwrk/midnight-ledger) produces when it applies a transaction. Each ledger event is deposited as a Substrate runtime event, so consumers read it through standard Substrate tooling instead of re-applying transactions locally.

## What is emitted

When a transaction is applied, the ledger emits a `Vec<Event>` describing the effects (Zswap inputs and outputs, contract deploys, contract logs, parameter changes, dust events). The node forwards each of these as one runtime event:

- `pallet_midnight::Event::LedgerEvent(LedgerEvent)` — for user transactions applied through `send_mn_transaction`.
- `pallet_midnight_system::Event::LedgerEvent(LedgerEvent)` — for system transactions (parameter changes, initial dust UTXOs, dust generation).

Both variants are appended last on their event enums, so the existing event variants and their indices are unchanged.

A `LedgerEvent` is:

```rust
pub struct LedgerEvent {
    pub source: LedgerEventSource,
    pub content_tagged_bytes: Vec<u8>,
}

pub struct LedgerEventSource {
    pub transaction_hash: [u8; 32],
    pub logical_segment: u16,
    pub physical_segment: u16,
}
```

`source` is the routing header — a SCALE mirror of the ledger's `EventSource` (tag `event-source[v1]`, stable across every ledger version the node links). It lets a consumer route on `(transaction_hash, logical_segment, physical_segment)` without decoding the payload.

`content_tagged_bytes` is the ledger's own tagged serialisation of the event's `EventDetails`. It is left opaque to the runtime: the runtime never needs to inspect event contents, and keeping the payload opaque means a future ledger upgrade that extends the event enum does not change this wire shape.

Only committed transactions emit events. A failed transaction emits none, a partially-successful transaction emits events only for the segments that succeeded, and dry-run / mempool-validation paths never emit events.

## Consuming events (indexer authors)

Read the events for a block from `frame_system::Events`, either by subscribing to storage changes or by reading the storage value at a finalised block hash:

- `state_subscribeStorage([System.Events])` for a live feed.
- `state_getStorage(System.Events, blockHash)` for a specific block.

`frame_system::Events` is cleared at the start of each block, so it holds only the events for the block being queried. Runtime events live in the state trie, not in the gossiped block body — every full node re-derives them locally by executing the block's extrinsics.

For each `LedgerEvent` record, decode `content_tagged_bytes` with the matching ledger version's `tagged_deserialize::<EventDetails>`. The tag is a self-describing byte-prefix: it identifies both the type and the ledger version that produced it (`event-details[v9]` for the v7/v8-era ledgers, `event-details[v14]` for the v9-era ledger). A version-aware consumer dispatches on the prefix and selects the matching decoder; the node applies no version logic of its own.

## Contract event namespacing (contract authors)

A contract event arrives as an `EventDetails::ContractLog` inside `content_tagged_bytes`, carrying the emitting contract's `address` and the `entry_point` that produced it. Combined with the `source` routing header, the `(address, entry_point)` pair is the namespace for a contract-emitted event: two contracts that emit under the same `entry_point` remain distinguishable by their `address`. There is no separate node-side topic field — the address and entry point already travel inside the event payload.

## Pricing

Event emission is deliberately left unpriced: `deposit_event` carries no per-event weight term and there is no event cap. This matches the upstream FRAME convention for `frame_system::Events` (whitelisted, unbounded storage that is excluded from weight benchmarking). Event volume is transitively bounded by the ledger's per-block synthetic-cost limits (`bytes_churned`), which the transaction fee already pays for, and midnight-node is not a parachain, so there is no proof-of-validity inflation concern.

The `bench_block_full_of_events` benchmark in `pallets/midnight/src/benchmarking.rs` is the guardrail for this decision: it fills a block with a worst-case event stream and measures the deposit cost against the block weight budget. If a future ledger version ever decouples event volume from state churn, the documented fallback is to add a per-event weight term to the transaction weight — a runtime-side change that does not affect the wire shape above.
