#runtime
# Add `SessionInfoApi` runtime API exposing the substrate session index

A new `midnight-primitives-session-info::SessionInfoApi` runtime API
exposes `current_session_index() -> u32`, backed by
`pallet_partner_chains_session::Pallet::current_index()`. Node-side code
can now query the session index via a typed API instead of reading the
pallet's storage directly.

Requires a metadata rebuild.

PR: https://github.com/midnightntwrk/midnight-node/pull/1534
Issue: https://github.com/midnightntwrk/midnight-node/issues/1399
