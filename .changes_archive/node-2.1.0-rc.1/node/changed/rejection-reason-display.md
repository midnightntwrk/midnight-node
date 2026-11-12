#logging #hygiene
# Log transaction rejection reasons via Display rather than Debug

The mempool, pre-dispatch and apply sinks rendered a rejected transaction's
`TransactionInvalid` with `{:?}`, which prints the error's full payload -
including any ledger `StateValue` embedded in it - instead of the one-line
diagnostic its `Display` impl already provides. The `PartialSuccess` sink did
the same for its whole per-segment result map.

Switch all of them to `Display` (the segment map is now rendered as
`[<segment>: <reason>, ...]`). Log lines become shorter and readable; the
node-side `InvalidError` code returned to callers is unchanged, so nothing
downstream of the log is affected.

PR: https://github.com/midnightntwrk/midnight-node/pull/2105
