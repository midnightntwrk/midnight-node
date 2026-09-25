#rpc #dependencies
# Switch polkadot-sdk to the shieldedtech fork of stable2606 with TCP_NODELAY on RPC sockets

The substrate JSON-RPC server accepts sockets in its own loop and never runs the
jsonrpsee accept path that sets `TCP_NODELAY`, so Nagle's algorithm stayed enabled
on every RPC connection and small messages such as `chainHead` notifications were
coalesced and delayed. The fix (paritytech/polkadot-sdk#12797) is only on master,
so all polkadot-sdk crates now come from the `stable2606` branch of
https://github.com/shieldedtech/polkadot-sdk, which is upstream `stable2606`
(stable2606-2) plus that commit. This also moves the node from the
`polkadot-stable2606` tag to the stable2606-2 crate versions.

PR: https://github.com/midnightntwrk/midnight-node/pull/2200
Issue: https://github.com/midnightntwrk/midnight-node/issues/2201
