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

A block's events are readable only while the node keeps that block's state. A node with default state pruning retains recent blocks only; serving historical events needs an archive node (`--state-pruning archive`), which is an operator opt-in.

There is no dedicated event-subscription RPC: consumers use the standard storage RPCs above. A typed `midnight_subscribeBlockEvents` wrapper is deliberate future work, to be added when a consumer that cannot decode `System.Events` through runtime metadata (a light client, a bridge, a raw JSON-RPC integrator) needs one.

For each `LedgerEvent` record, decode `content_tagged_bytes` with the matching ledger version's `tagged_deserialize::<EventDetails>`. The tag is a self-describing byte-prefix: it identifies both the type and the ledger version that produced it (`event-details[v9]` for the ledger-8 era, `event-details[v14]` for the ledger-9 era). A version-aware consumer dispatches on the prefix and selects the matching decoder; the node applies no version logic of its own.

`EventDetails` derives the ledger's `Storable`, not a flat `Serializable`, so its tagged bytes are a topologically sorted arena node list rather than a struct-shaped record. The encoding is self-contained, but decoding allocates the nodes into a local arena: a consumer must link the ledger's storage crates (`midnight-storage`, `midnight-storage-core`) alongside the ledger crate, and cannot decode the payload with a SCALE codec or mirror it as a flat runtime type.

## Contract event namespacing (contract authors)

A contract event arrives as an `EventDetails::ContractLog` inside `content_tagged_bytes`, carrying the emitting contract's `address` and the `entry_point` that produced it. Combined with the `source` routing header, the `(address, entry_point)` pair is the namespace for a contract-emitted event: two contracts that emit under the same `entry_point` remain distinguishable by their `address`. There is no separate node-side topic field — the address and entry point already travel inside the event payload.

## Pricing

User-transaction events carry no per-event weight term. Their volume is transitively bounded by the ledger's per-block synthetic-cost limits (`bytes_churned`), which the transaction fee already pays for, matching the upstream FRAME convention for `frame_system::Events` (whitelisted storage excluded from weight benchmarking).

System-transaction events are weighed per event, because those paths pay no fee and so are not bounded by it:

- `send_mn_system_transaction` declares `ConfigurableSystemTxWeight` plus `PER_LEDGER_EVENT_WEIGHT × MAX_SYSTEM_TX_LEDGER_EVENTS` before dispatch, and refines it to the events actually deposited after.
- `pallet_cnight_observation::process_tokens` adds `WeightInfo::ledger_event_deposit(n)`, bounded before dispatch by the UTXO capacity and refined to the actual count after.

The per-event figures are conservative placeholders. The `bench_block_full_of_events` benchmark in `pallets/midnight/src/benchmarking.rs` fills a block with worst-case-sized events up to the 50 MB `bytes_churned` ceiling and measures the deposit cost; its result replaces them.
