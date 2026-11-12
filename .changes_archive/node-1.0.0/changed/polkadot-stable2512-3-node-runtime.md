#node #runtime
# Align node and runtime with polkadot-stable2512-3 SDK

Bumps Substrate dependencies to the `polkadot-stable2512-3` tag and updates call sites for breaking API changes: `Core::execute_block` and `BlockBuilder::check_inherents` now use `LazyBlock`; `SpawnTasksParams` requires `tracing_execute_block` (set to `None` unless trace RPC is wired); `MmrApi` v3 gains `generate_ancestry_proof` while `BeefyApi` no longer exposes it; pallet-version test mock implements `Core` with `LazyBlock`. Partner-chains and lockfiles are updated in line with the same SDK line.

PR: https://github.com/midnightntwrk/midnight-node/pull/1262
Required for https://github.com/midnightntwrk/midnight-node/issues/1244