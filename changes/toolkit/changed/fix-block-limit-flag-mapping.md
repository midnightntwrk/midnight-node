# Fix update-ledger-parameters block-limit flag mapping

`update-ledger-parameters` now maps each `--block-limit-*` flag to its own
field. Previously all five block limits were populated from
`--block-limit-read-time`: the other four flags were parsed but silently
ignored, and setting only the read-time limit overwrote every block limit
with the same value.

Issue: https://github.com/midnightntwrk/midnight-node/issues/1904
