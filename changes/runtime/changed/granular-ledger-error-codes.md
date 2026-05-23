#runtime
# Expose granular ledger error variants in pallet error reporting

Adds new variants to `InvalidError`, `MalformedError`, and
`SystemTransactionError` so failed transactions report the specific inner
cause instead of a single catch-all (e.g. EffectsCheckFailure now reports
which of the seven EffectsCheckError variants triggered the rejection).
Existing error codes are unchanged; new variants occupy stable u8 codes in
the 212-250 range. Runtime metadata rebuild required.

Helps with: https://github.com/midnightntwrk/midnight-node/issues/1374
PR: https://github.com/midnightntwrk/midnight-node/pull/1449
