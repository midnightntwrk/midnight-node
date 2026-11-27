#node #runtime
# Check block inherents in the on-chain context via the Polkadot SDK

polkadot-stable2611 includes paritytech/polkadot-sdk#13840: `sp_block_builder::check_inherents`
calls the `check_inherents` runtime API in the on-chain context and runs `initialize_block`
first, so the first block after a runtime upgrade is checked by the runtime that built it, after
its migrations ran.

This replaces the local workaround from #2252, which is reverted: the Partner Chains
`check_inherents_on_chain` helper goes back to `sp_block_builder::check_inherents_with_data`,
and the runtime's `check_inherents` no longer runs `execute_on_runtime_upgrade` itself.

PR: <link to PR>
