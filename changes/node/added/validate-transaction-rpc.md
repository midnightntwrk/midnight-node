#node #runtime
# Add midnight_validateTransaction RPC endpoint

Adds a read-only `midnight_validateTransaction` JSON-RPC method that runs a
hex-encoded transaction through the ledger's verbose validation without
submitting it to the txpool. On success it returns the tx hash. On failure it
returns structured error details that `state_call` cannot provide:

- `-32001`: validation failure, with `data.{error_code, reason, details}`
  carrying the underlying ledger error (e.g. `ContractNotPresent`).
- `-32602`: invalid hex input.
- `-32601`: the node's runtime predates `MidnightRuntimeApi` v6, so it does not
  expose the validation context.
- `-32603`: internal error fetching the validation context from the runtime.

The validation context (ledger state key, block context, runtime spec version
and max block weight) comes from a new `get_validation_context` method on
`MidnightRuntimeApi` (bumped to `#[api_version(6)]`). Reading it through the
runtime API, rather than from hard-coded storage paths in the node, keeps the
layout compile-checked and guarantees all fields are read at the same block.
`max_block_weight` is enforced via the ledger cost check, which rejects
transactions that overflow block limits.

The endpoint has no rate limiting of its own. It costs about as much as
validating a transaction through `state_call`, so the node's general RPC
limits (`--rpc-rate-limit` and related flags) cover it.

PR: https://github.com/midnightntwrk/midnight-node/pull/867
Issue: https://github.com/midnightntwrk/midnight-node/issues/1197
