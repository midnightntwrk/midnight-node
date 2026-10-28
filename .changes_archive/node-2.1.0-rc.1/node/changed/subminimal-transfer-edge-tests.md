#tests

# Edge-case tests for c2m-bridge subminimal-transfer accumulation

Add unit tests for `handle_subminimal_transfer` covering: the strict `sum > threshold` boundary
(below / at / above), accumulator reset and restart after a flush, subminimal routing precedence
over Invalid / unapproved User recipients, and non-interference between regular and subminimal
transfers.

PR: https://github.com/midnightntwrk/midnight-node/pull/1677
Issue: https://github.com/midnightntwrk/midnight-node/issues/1248
