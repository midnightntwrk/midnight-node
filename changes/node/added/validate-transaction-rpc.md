#node
# Add midnight_validateTransaction RPC endpoint

Adds a read-only `midnight_validateTransaction` JSON-RPC method that runs a
hex-encoded transaction through the ledger's verbose validation without
submitting it to the txpool. On success it returns the tx hash; on failure
it returns one of three structured JSON-RPC errors:

- `-32602` — invalid hex input (rejected before any rate-limit bucket is
  touched).
- `-32001` — validation failure, with structured `data.{error_code, reason,
  details}` surfacing the underlying ledger error (e.g. `ContractNotPresent`).
- `-32005` — rate limited. The global call-rate quota is checked first (so a
  saturated node rejects without growing the keyed store), then the
  per-transaction cooldown (keyed by `blake2_256` of the tx bytes). The keyed
  store is bounded by periodically evicting keys whose cooldown has elapsed.
- `-32603` — internal error if the validation-context storage
  (`Midnight::StateKey`, `Midnight::ParentTimestamp`, `Timestamp::Now`) is
  missing, e.g. after a runtime/RPC storage-layout mismatch. Fails loudly
  rather than validating against a zeroed context.

Rate limits are configurable via `ValidateRateLimitConfig` (`res/cfg`):
`global_rate_limit` (default 50 calls/sec), `per_tx_cooldown_secs` (default
30s), and `max_block_weight` (enforced via the ledger cost check, rejecting
transactions that overflow block limits). Backed by a new
`validate_transaction_verbose` entry point in the ledger native API.

PR: https://github.com/midnightntwrk/midnight-node/pull/867
Issue: https://github.com/midnightntwrk/midnight-node/issues/1197
