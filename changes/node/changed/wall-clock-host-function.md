#node
# Register `wall_clock::now_millis` host function

Adds `midnight_node_ledger::host_api::clock::wall_clock::HostFunctions` to the node's executor
`HostFunctions` tuples (both benchmark and non-benchmark). This exposes a wall-clock timestamp
to the runtime, used by `pallet_midnight`'s `validate_unsigned` to base the DUST validity
window on real time.

Rollout note: a node must have this host function registered before running a runtime that
calls it. An old node importing/validating with a new runtime would fail to resolve the
`wall_clock::now_millis` wasm import — upgrade nodes with (or before) the runtime.

PR: https://github.com/midnightntwrk/midnight-node/pull/1877
Issue: https://github.com/midnightntwrk/midnight-node/issues/1856
