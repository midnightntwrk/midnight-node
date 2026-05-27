#node #runtime
# Add midnight_validateTransaction RPC endpoint

Adds a read-only `midnight_validateTransaction` JSON-RPC method that runs a
hex-encoded transaction through the ledger's verbose validation without
submitting it to the txpool. On success it returns the tx hash; on failure
it returns one of these structured JSON-RPC errors:

- `-32602` — invalid hex input (rejected before any rate-limit bucket is
  touched).
- `-32001` — validation failure, with structured `data.{error_code, reason,
  details}` surfacing the underlying ledger error (e.g. `ContractNotPresent`).
- `-32005` — rate limited. The per-transaction cooldown (keyed by `blake2_256`
  of the tx bytes) is checked before the global call-rate quota, so a client
  replaying the same transaction is rejected without consuming a global token
  and cannot drain the shared budget for other callers. The keyed store is
  bounded by size: keys whose cooldown has elapsed are evicted once the store
  grows past a fixed threshold.
- `-32601` — the node's runtime predates `MidnightRuntimeApi` v6 and so does
  not expose the validation context.
- `-32603` — internal error fetching the validation context from the runtime.

The validation context (ledger state key, block context, runtime spec version,
and max block weight) is served by a new `get_validation_context` method on
`MidnightRuntimeApi` (bumped to `#[api_version(6)]`), returning the
`ValidationContext` struct. Reading it through the runtime API — rather than
from hardcoded storage paths in the node — keeps the layout change
compile-checked and guarantees all fields are read at the same block.
`max_block_weight` is enforced via the ledger cost check, rejecting
transactions that overflow block limits.

Rate limits are configurable via `ValidateRateLimitConfig` (`res/cfg`):
`global_rate_limit` (default 50 calls/sec) and `per_tx_cooldown_secs` (default
30s). Backed by a new `validate_transaction_verbose` entry point in the ledger
native API.

PR: https://github.com/midnightntwrk/midnight-node/pull/867
Issue: https://github.com/midnightntwrk/midnight-node/issues/1197
