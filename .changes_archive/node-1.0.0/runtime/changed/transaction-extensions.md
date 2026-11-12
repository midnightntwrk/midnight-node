#runtime
# Migrate from SignedExtension to TransactionExtension

Migrates the runtime from the deprecated `SignedExtra` type alias to the new
`TxExtension` pattern, adding `AuthorizeCall` and `WeightReclaim` extensions.
Implements the required offchain transaction creation traits
(`CreateTransaction`, `CreateBare`, `CreateSignedTransaction`,
`CreateAuthorizedTransaction`) and updates the benchmarking harness to match.

PR: https://github.com/midnightntwrk/midnight-node/pull/597
