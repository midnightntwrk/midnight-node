#rpc #dependencies
# Switch polkadot-sdk to the shieldedtech fork of stable2606 with TCP_NODELAY on RPC sockets

The substrate JSON-RPC server accepts sockets in its own loop and never runs the
jsonrpsee accept path that sets `TCP_NODELAY`, so Nagle's algorithm stayed enabled
on every RPC connection and small messages such as `chainHead` notifications were
coalesced and delayed. The fix (paritytech/polkadot-sdk#12797) is only on master,
so all polkadot-sdk crates now come from the `stable2606` branch of
https://github.com/shieldedtech/polkadot-sdk, which is the upstream
`polkadot-stable2606` tag plus that single commit. Crate versions are unchanged;
only the dependency source moves, and `Cargo.lock` pins the fork revision.

PR: https://github.com/midnightntwrk/midnight-node/pull/2200
Issue: https://github.com/midnightntwrk/midnight-node/issues/2201
