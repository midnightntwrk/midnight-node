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

PR: https://github.com/midnightntwrk/midnight-node/pull/1980
Issue: https://github.com/midnightntwrk/midnight-node/issues/1979
