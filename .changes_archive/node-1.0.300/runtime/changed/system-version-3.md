#runtime
# Bump `system_version` to 3

Raise `RuntimeVersion::system_version` from 1 to 3 so the runtime opts into
the FRAME pending-code upgrade path (RFC-123). That keeps off-chain Runtime
API calls from running new Wasm against unmigrated storage after
`set_code` ([paritytech/polkadot-sdk#64](https://github.com/paritytech/polkadot-sdk/issues/64),
[paritytech/polkadot-sdk#6029](https://github.com/paritytech/polkadot-sdk/pull/6029)).

PR: https://github.com/midnightntwrk/midnight-node/pull/1900
Issue: https://github.com/midnightntwrk/midnight-node/issues/1901
