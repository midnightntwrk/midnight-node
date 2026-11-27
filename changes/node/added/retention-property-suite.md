#node #ledger #tests

# Property tests for keeping intra-block ledger states alive

A proptest suite checks that block execution keeps every ledger state the node may still
load, and keeps nothing else. It runs random histories through the runtime: blocks built
with `frame_executive::Executive` over pallet-midnight-system's mock, applying cNIGHT system
transactions and user transactions, rolling back, finalizing or abandoning blocks, and
off-chain runtime API calls, with executions interleaved. After each call it checks that
live keys resolve and that the only GC roots are tips. After each `finalize_block` it checks
that released states are gone and that the disk holds only what the roots reach. Every call
is also compared, byte for byte, against a model world that never releases a state.

The two worlds run side by side through a new, non-default `suite-storage` feature of
`midnight-node-ledger`, which lets a test choose the ledger host functions' storage per
execution. The node binary does not build it.

The properties and every test of them are in
`pallets/midnight-system/src/retention_suite/properties.rs`. Longer histories run nightly
through `earthly +test-retention-nightly`. An ignored test,
`flush_while_another_execution_holds`, reproduces a known leak: a block's flush writes
another block's held states to disk with no root.

PR: https://github.com/midnightntwrk/midnight-node/pull/2531
Issue: https://github.com/midnightntwrk/midnight-node/issues/2530
