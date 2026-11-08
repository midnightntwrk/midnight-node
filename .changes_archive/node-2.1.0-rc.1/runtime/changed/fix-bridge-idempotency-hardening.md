#runtime #c2m-bridge

# Handling ledger transaction execution errors

The bridge will not move checkpoint when transaction fails execution or serialization.
PC bridge pallet will retry it again in the next block. If it keeps failing, it perhaps
means there is a bug that has to be fixed. Bridge will stop processing at this point.

Future-proof single variant enum (SCALE-friendly, but not Rust idiomatic) error is introduced in case we would like to skip on some errors in the future.

One Cardano transaction yields at most two transfers: a _reserve transfer_, made when it takes
tokens out of the reserve, followed by an _ICS transfer_ of the remaining tokens it locked in the
illiquid circulating supply. Processing can therefore stop at one place inside a Cardano transaction,
which the new `BridgeDataCheckpoint::TxReserveTransfer` variant denotes.
The kind of a transfer's recipient identifies which of the two transfers of its transaction it is,
so `BridgeTransferV1::checkpoint` derives the checkpoint of a handled transfer without any
bookkeeping. `handle_transfers` uses it to record the last transfer it handled, and leaves the checkpoint
untouched when it handled none.

Because a checkpoint can now point between the two transfers of a transaction, the observability
layer no longer has to return them together, and cuts the transfers it returns at the configured
limit wherever that falls. Any `MaxTransfersPerBlock` of at least one guarantees that processing
progresses; the value only bounds throughput and the weight of the inherent.

PR: https://github.com/midnightntwrk/midnight-node/pull/1980
Issue: https://github.com/midnightntwrk/midnight-node/issues/1979
