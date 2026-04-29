#node
# Surface nested ledger error variants in flat error enums

Variants like `MalformedTransaction::EffectsCheckFailure(EffectsCheckError)`
were previously collapsed to a single flat code, hiding the inner cause from
end-users. The host-side conversions now flatten the nested enums
(EffectsCheckError, SequencingCheckError, DisjointCheckError,
FeeCalculationError, MalformedContractDeploy, TransactionApplicationError,
zswap MalformedOffer and TransactionInvalid) into granular variants in
InvalidError, MalformedError, and SystemTransactionError, with stable u8 codes
in the previously free 212-250 range. Adds variants for ledger 8's
MerkleTreeError (top-level and zswap-nested) and DivideByZero. Version-specific
conversions live in versions/error_ext/ledger_{7,8}.rs so future ledger
upgrades can extend mappings without touching shared code, and unknown
variants now log a warning instead of being silently misclassified.

Helps with: https://github.com/midnightntwrk/midnight-node/issues/1374
PR:
