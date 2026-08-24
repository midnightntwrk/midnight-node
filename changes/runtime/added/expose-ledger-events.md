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
transactions. Emission is non-consensus and unpriced; see `docs/ledger-events.md`.

Requires a metadata rebuild.

PR: https://github.com/midnightntwrk/midnight-node/pull/1849
Issue: https://github.com/midnightntwrk/midnight-node/issues/1474
