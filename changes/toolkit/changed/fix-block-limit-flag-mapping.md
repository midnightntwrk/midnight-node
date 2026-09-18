#toolkit #test
# Extract block-limit flag mapping into a tested helper

`update-ledger-parameters` block-limit mapping is extracted into an
`apply_block_limit_updates` helper with regression tests. The underlying
bug — all five block limits populated from `--block-limit-read-time` while
the other four flags were silently ignored — was fixed in #2073; the tests
here pin each `--block-limit-*` flag to its own field and verify that unset
flags keep the base values, so the mapping cannot regress silently again.

Issue: https://github.com/midnightntwrk/midnight-node/issues/1904

PR: https://github.com/midnightntwrk/midnight-node/pull/1915
