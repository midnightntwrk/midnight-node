#node #runtime #c2m-bridge

# Handling ledger transaction execution errors

The bridge will not move checkpoint when transaction fails execution or serialization.
PC bridge pallet will retry it again in the next block. If it keeps failing, it perhaps
means there is a bug that has to be fixed. Bridge will stop processing at this point.

Future-proof enum (SCALE-friendly) error is introduced in case we would like to skip on some errors in the future.

The exectly-once processing is quite complex because we still don't know if Cardano validator would prevent
transactions that release from unlocked and reserve to ICS at the same time.

A very unlikely corner case when bridge max transfers is configured to 1 and transaction mapped
to two transfers is encountered has be reimplemented to stuck instead of silently skipping all
transfer up to given block bound.

Before handling them, `handle_transfers` pairs every transfer with its index among the transfers of
its Cardano transaction, counted over the whole transaction: for the transaction the current
checkpoint points at, the count continues from the transfers already handled in an earlier block,
which the observability layer leaves out of the data. That index is the number of transfers of the
transaction that precede the one being handled.

On a failure the checkpoint is therefore always a `PartialTx` of the failing transfer's Cardano
transaction, with `transfers_processed` taken from that index — zero when the failing transfer is
the first one of its transaction. `PartialTx` is inclusive of the transaction it points at, so this
is the same resume point that a `Tx` checkpoint of the preceding transaction expressed, and the
pallet no longer has to track which transactions were completed earlier in the block.

PR: https://github.com/midnightntwrk/midnight-node/pull/1980
Issue: https://github.com/midnightntwrk/midnight-node/issues/1979
