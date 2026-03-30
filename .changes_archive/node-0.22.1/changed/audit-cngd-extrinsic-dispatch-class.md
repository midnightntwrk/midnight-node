#audit #runtime
# Add benchmarked weight and UTXO count validation to process_tokens

The `process_tokens` inherent extrinsic previously declared zero weight with
`DispatchClass::Mandatory`, bypassing block weight accounting. This adds FRAME
benchmarking infrastructure, a runtime UTXO count guard (`TooManyUtxos`), and
`PostDispatchInfo` actual-weight correction to the cnight-observation pallet.

PR: https://github.com/midnightntwrk/midnight-node/pull/798
Ticket: https://shielded.atlassian.net/browse/PM-19778
