#runtime #ledger
# Error codes for the ledger 9.1.0.1 rejections

`pallet-midnight`'s ledger-9 error enums gain codes for the new rejections instead of
reporting them as `UnknownError`: `InvalidError::{BridgeClaimTooSmall,
ContractTooManyOperations}`, `MalformedError::{TooManyPublicInputs,
TranscriptGasExceedsBlockLimits, ContractDustTransfer, RepeatedCommitteeSignature,
RepeatedDustRegistration}`, `FeeCalculationErrorCode::{RepeatedEntries, Unknown}` and
`MalformedContractDeployErrorCode::TooManyOperations`. Existing codes keep their numbers.

Runtime metadata needs rebuilding (`/bot rebuild-metadata`).

PR: TBD
