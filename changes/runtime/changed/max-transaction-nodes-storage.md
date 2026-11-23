#runtime #ledger #governance
# Raise the transaction node cap to 8,192 and make it governable

`MaxTransactionNodes` is now a `pallet-midnight` storage value, defaulting to 8,192
(was a 1,024 config constant). Root can change it with `set_max_transaction_nodes`
without a runtime upgrade. The setter refuses values that would admit more operations
than `MaxDecodedTxItems`, so the node cap always binds first.

1,024 refused 54 transactions already accepted on preview, preprod and mainnet (largest:
2,023 nodes) and a compactc-generated contract call declaring 6,287 nodes.

PR: https://github.com/midnightntwrk/midnight-node/pull/2411
Issue: https://github.com/midnightntwrk/midnight-node/issues/2386
