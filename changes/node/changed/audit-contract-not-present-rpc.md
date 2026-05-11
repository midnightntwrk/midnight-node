#audit #rpc
# Surface ContractNotPresent through midnight_contractState RPC

The runtime returned `Err(LedgerApiError::ContractNotPresent)` for queries against
undeployed contract addresses since PR #916, but the `midnight_contractState` RPC
collapsed every `LedgerApiError` into the generic `UnableToGetContractState`,
preventing callers from distinguishing "contract has empty state" from "no such
contract". This adds a new `StateRpcError::ContractNotPresent` variant and routes
the matching ledger error to it; all other variants still fall through to
`UnableToGetContractState`, so the change is a strict superset of prior behaviour.

Closes: https://github.com/midnightntwrk/midnight-node/issues/1166
PR: https://github.com/midnightntwrk/midnight-node/pull/1475
