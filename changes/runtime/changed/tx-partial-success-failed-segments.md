#runtime #node #events #indexer

# `TxPartialSuccess` reports which segments failed, and why

`pallet_midnight::Event::TxPartialSuccess` now carries `TxPartialSuccessDetails { tx_hash,
failed_segments }`. `failed_segments` is a `Vec<(u16, SegmentFailure)>` of each fallible segment
that did not apply. Every segment not listed did apply. An indexer reading node events can
therefore decide which fallible offers to apply without re-running the ledger.

`SegmentFailure` is a one-byte code naming the top-level ledger `TransactionInvalid` variant
(`Transcript`, `Zswap`, `ContractNotPresent`, ...), with `Other` for variants this node does not
map. It does not include the full error: that is unbounded and not guaranteed to be identical
across nodes. To get it, re-run the segment off-chain.

`TxApplied` is unchanged. This is a breaking metadata change for decoders of `TxPartialSuccess`.

The runtime calls the new `apply_transaction` host function v4, so nodes must upgrade before
this runtime is enacted. Versions 1–3 are unchanged, so historical blocks still replay.

PR: https://github.com/midnightntwrk/midnight-node/pull/2492
Issue: https://github.com/midnightntwrk/midnight-node/issues/2491
