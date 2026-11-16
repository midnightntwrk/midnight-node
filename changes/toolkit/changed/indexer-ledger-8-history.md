#toolkit #indexer
# Sync wallets over the indexer on chains that forked from ledger 8

On a chain that started on ledger 8 and forked to ledger 9 (mainnet), the indexer serves pre-fork
transactions and DUST events in ledger-8 encoding, which the ledger-9 indexer sync failed to decode.

Pre-fork shielded transactions are now applied to a ledger-8 zswap wallet whose state is carried
into the ledger-9 wallet at the first ledger-9 transaction, as the block-replay path does in
`fork_context_8_to_9`. Pre-fork DUST events are skipped, since the fork resets DUST state.

Tested on mainnet through Blockfrost: `show-wallet` and unshielded and shielded `generate-txs
single-tx` sends to self, all applied with `TxApplied`.

PR: https://github.com/midnightntwrk/midnight-node/pull/2397
Issue: https://github.com/midnightntwrk/midnight-node/issues/1186
