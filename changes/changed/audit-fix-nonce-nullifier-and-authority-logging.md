#toolkit
# Cleanup nullifier/nonce use in fork export path and redact new_authority in CLI logging

Fix remaining nullifier/nonce use in the fork-aware export path
 where the nullifier was incorrectly serialized as the nonce
in EncodedShieldedCoinInfo. This mirrors the fix from PR #895.

Also convert the maintain-contract new_authority parameter from a positional CLI
argument to a named flag (--new-authority).

PR: https://github.com/midnightntwrk/midnight-node/pull/1074
