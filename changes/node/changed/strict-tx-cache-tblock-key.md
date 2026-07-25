#ledger
# Include tblock in the strict transaction-validation cache key

The strict transaction-validation cache in `midnight-node-ledger` was keyed only by
`(state_hash, tx_hash)`, but the cached `well_formed` verdict also depends on `tblock` (a
transaction is malformed with `OutOfDustValidityWindow` when its `ctime > tblock`). The mempool
validation path in `pallet_midnight` deliberately inflates `tblock` by up to
`MaxSkippedSlots + 1` slots to avoid prematurely dropping soon-to-be-valid transactions; because
the cache lookup ignored `tblock`, that inflated "valid" verdict then leaked into block
application, which uses the true, lower block-time `tblock`. As a result the first transaction in
a block could be accepted by a cache-warm authoring node yet rejected with
`OutOfDustValidityWindow` by a cache-cold importer or indexer — a consensus divergence that
caused Preview sync stalls. Adding `tblock` to `StrictTxValidationKey` keeps the mempool-time and
block-application verdicts separate, so block application and import always evaluate `well_formed`
at the true block time.

PR: <link to PR>
Issue: https://github.com/midnightntwrk/midnight-node/issues/1924
