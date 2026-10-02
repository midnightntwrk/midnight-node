#runtime
# Expose ledger events as runtime events

The runtime now surfaces the per-transaction event stream that the ledger
produces when it applies a transaction. Each ledger event is deposited as a
Substrate runtime event: `pallet_midnight::Event::LedgerEvent` for user
transactions and `pallet_midnight_system::Event::LedgerEvent` for system
transactions. Both variants are appended last, so the existing event variants
and their indices are unchanged.

A `LedgerEvent` carries a SCALE routing header (`transaction_hash`,
`logical_segment`, `physical_segment`) plus the ledger's own tagged
serialisation of the event details as opaque bytes, so the wire shape is stable
across ledger versions. Consumers read events from `frame_system::Events` via
`state_subscribeStorage` / `state_getStorage` instead of re-applying
transactions. System-transaction events are weighed per event; see
`docs/ledger-events.md`.

The events cross the host boundary on new host-function versions:
`apply_transaction` v3 (ledger 8) and v2 (ledger 9), and v2 of
`apply_governance_system_transaction`, `apply_cnight_system_transaction` and
`apply_bridge_system_transaction`. The earlier versions keep their events-free
return shape. Validator binaries must provide these host functions before the
runtime is activated.

Requires a metadata rebuild.

PR: https://github.com/midnightntwrk/midnight-node/pull/1849
Issue: https://github.com/midnightntwrk/midnight-node/issues/1474
