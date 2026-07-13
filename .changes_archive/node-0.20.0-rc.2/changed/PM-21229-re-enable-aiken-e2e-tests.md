# Re-enable Aiken E2E Tests

## Summary

Re-enable all Aiken E2E tests by fixing the policy ID mismatch between
partner-chains contracts and Aiken governance contracts.

Key fixes:
- Remove SKIP_GOVERNANCE_DEPLOY flag from Earthfile, docker-compose.yml, and entrypoint.sh
- Governance contracts are now always deployed by midnight-setup during local-env startup
- **Critical fix**: Chain-spec now uses Aiken council_forever policy ID instead of
  partner-chains PermissionedCandidates policy ID. This ensures nodes look for
  permissioned candidates at the correct contract address.
- compile-contracts.sh now writes policy IDs to runtime-values files
- entrypoint.sh reads and uses Aiken policy IDs when generating chain-spec
- Convert `deploy_governance_contracts_and_validate_membership_reset` to verify already-deployed contracts
- Convert `deploy_federated_ops_contract_and_validate_membership` to verify already-deployed contract
- Add `query_utxos` method to CardanoClient for contract verification
- Re-enable 14 tests that were previously ignored due to chain observation issues

## Issue

Fixes [PM-21229](https://shielded.atlassian.net/browse/PM-21229)

## PR

https://github.com/midnightntwrk/midnight-node/pull/471

## Type

Bug Fix
