#node #host-function #events

# `apply_transaction` host function v4

Adds ledger-9 `apply_transaction` v4. It reports each failed segment's id and `SegmentFailure`
code, which the runtime uses for `TxPartialSuccess` events. A runtime that binds v4 can't
execute on a node without it, so upgrade nodes before that runtime is enacted. v1–v3 are
unchanged.

PR: https://github.com/midnightntwrk/midnight-node/pull/2492
Issue: https://github.com/midnightntwrk/midnight-node/issues/2491
