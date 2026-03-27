#toolkit
# Restore untagged decoding for contract-address and coin-public inputs

The contract-address and coin-public CLI inputs are intentionally untagged.
Reverts the tagged-only change for these two inputs while keeping tagged
decoding as the default for other ledger types.

PR: https://github.com/midnightntwrk/midnight-node/pull/853
