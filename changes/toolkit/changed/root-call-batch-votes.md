#toolkit
# Batch council / technical-committee votes in root_call governance

`root_call` previously cast each committee vote serially, waiting for each to finalize
before submitting the next. Votes come from distinct signers (independent nonces) and
the preceding `propose` is already finalized, so they are now submitted back-to-back and
awaited together — landing and finalizing in the same block(s) instead of one per block.
Cuts governance-round latency (e.g. local-env midnight-setup).

PR: https://github.com/midnightntwrk/midnight-node/pull/1796
