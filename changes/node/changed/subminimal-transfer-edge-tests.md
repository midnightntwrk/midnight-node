#tests

# Edge-case tests for c2m-bridge subminimal-transfer accumulation

Add unit tests for `handle_subminimal_transfer` covering: the strict `sum > threshold` boundary
(below / at / above), a single subminimal that flushes immediately, accumulator reset and
restart after a flush, subminimal routing precedence over Invalid / unapproved User recipients,
governance-driven threshold lowering against a non-empty accumulator, and non-interference
between regular and subminimal transfers.

PR: https://github.com/midnightntwrk/midnight-node/pull/1677
Issue: https://github.com/midnightntwrk/midnight-node/issues/1248
