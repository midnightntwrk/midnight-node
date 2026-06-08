#node
# Use ledger 9 `apply_guaranteed_only` for guaranteed-segment validation

Mempool validation (`validate_transaction`) and block-inclusion pre-checks
(`validate_guaranteed_execution` / `pre_dispatch`) now dry-run only the
guaranteed transaction segment on ledger 9 via `apply_guaranteed_only`, instead
of a full `apply()` dry-run that also executed the fallible segment.

Ledger 7 and 8 keep the previous full-`apply()` dry-run path via new
version-specific `guaranteed_validation` modules (same pattern as `system_tx`
and `error_ext`).

PR: https://github.com/midnightntwrk/midnight-node/pull/1454
