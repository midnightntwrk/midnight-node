#toolkit
# Return errors instead of panicking in single-tx and fetcher

`generate-txs single-tx` panicked when the source wallet had insufficient
shielded coins or unshielded UTXOs. These are expected runtime conditions for
a CLI, so they now surface as structured errors (`SingleTxError`, mirroring
`BatchSingleTxError`) with a clean non-zero exit; proving failures and empty
transactions are converted the same way. Also saturate the height-span
arithmetic in `fetch_from_rpc` (job sizing, progress log, and summary log),
which underflowed (debug-build panic) when the queried node's finalized height
was below the cached watermark. Found by Antithesis fault testing.

Closes: #1821
PR: https://github.com/midnightntwrk/midnight-node/pull/1822
