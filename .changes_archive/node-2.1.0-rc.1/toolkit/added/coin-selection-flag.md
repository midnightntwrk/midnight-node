#toolkit
# Add `--coin-selection` flag to coin-selecting commands

`generate-txs single-tx`, `generate-txs batches`, and `generate-txs batch-single-tx`
now accept `--coin-selection <largest-first|smallest-first>` to control how candidate
coins/UTXOs are ordered during input selection.
`largest-first` (the default) minimizes the number of inputs.
`smallest-first` consolidates dust by spending the smallest coins/UTXOs first.

PR: https://github.com/midnightntwrk/midnight-node/pull/1457
Issue: https://github.com/midnightntwrk/midnight-node/issues/1456
