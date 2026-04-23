#node
# Return ContractNotPresent error for missing contracts

Return an explicit ContractNotPresent error when querying the state of a non-existent contract address, instead of returning a default empty state. This allows callers to distinguish between an empty contract and a missing one.

Closes: midnightntwrk/midnight-node#1166
PR: https://github.com/midnightntwrk/midnight-node/pull/916
