# Re-enable Aiken E2E Tests

## Summary

Re-enable all Aiken E2E tests by removing the SKIP_GOVERNANCE_DEPLOY infrastructure
and converting deployment tests to verification tests.

Changes:
- Remove SKIP_GOVERNANCE_DEPLOY flag from Earthfile, docker-compose.yml, and entrypoint.sh
- Governance contracts are now always deployed by midnight-setup during local-env startup
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
