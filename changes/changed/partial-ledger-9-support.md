#node #runtime #toolkit

# Ledger 9 support

Adds Ledger 9 dependencies. Allows running new chains with ledger 9 on local-environment.

- Removed ledger v8 function 'construct_distribute_treasury_system_tx' that was invoked (incorrectly) only in c2m-bridge pallet.
  Since the bridge is not enable, it never happened and is safe to be removed.
- Approach to supporting mixed chains (replay blocks, toolkit caching) of pre v9 and post v9 chains is very relaxed, as it is not yet supported.
- Ignored tests that require `intent[v7]` or hard-fork/migration from ledger v8 to v9.
- Ignored failures of some CI jobs that depend on toolkit-js or hard-fork.

Ticket: https://github.com/midnightntwrk/midnight-node/issues/1579
PR: https://github.com/midnightntwrk/midnight-node/pull/1604
