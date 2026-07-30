#audit #ledger
# Remove stale cost model stubs and re-enable integration test

Remove the `// TODO COST MODEL:` comment and `#[allow(unused_variables)]`
annotation left over from the original stub implementation of
`get_transaction_cost`, and prefix the unused `block_context` parameter with
an underscore. Re-enable the `test_get_mn_transaction_fee` integration test
that was ignored while the function was still a stub.

PR: https://github.com/midnightntwrk/midnight-node/pull/839
Ticket: https://shielded.atlassian.net/browse/PM-19968
