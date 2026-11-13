#toolkit #fix
# Skip Midnight transactions whose extrinsic failed when replaying the chain

Chain replay (`show-wallet` and every other command that replays blocks) panicked with
`StateRootMismatch` at a block containing a `send_mn_transaction` extrinsic that failed dispatch,
e.g. one filtered with `CallFiltered` while safe mode was entered. The node rolls back a failed
extrinsic's storage, so its ledger never applies the transaction, but the toolkit replayed it
anyway. The fetcher now drops Midnight transactions whose extrinsic emitted
`System.ExtrinsicFailed`.

PR: https://github.com/midnightntwrk/midnight-node/pull/2443
Issue: https://github.com/midnightntwrk/midnight-node/issues/2441
