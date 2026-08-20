# Indexer Test Execution Report

## Run Details

| Field | Value |
| --- | --- |
| Component Under Test | Indexer / Node |
| Indexer Version | 4.3.3 |
| Node Version | 1.0.0 |
| Test Suites | e2e |
| Tests | Indexer GraphQL API and integration with Node |
| Environment | undeployed (INDEXER_TAG=4.3.3, NODE_TAG=1.0.0) |
| Date | Friday 5th Jun 2026 |
| QA Contact | Giuseppe Salvatore (giuseppe.salvatore@shielded.io) |

## Summary

| Suite | Test Files | Passed | Failed | Skipped | Total | Duration | Run at |
| --- | --- | ---: | ---: | ---: | ---: | --- | --- |
| e2e | — | 38 | 0 | 0 | 38 | — | 9:09 AM BST |
| **Total** | | **38** | **0** | **0** | **38** | | |

> Legend: ✓ passed · ✗ failed · ↓ skipped.

## E2e results

### Contract actions sequential

File name: `tests/e2e/contract-actions-sequential.test.ts`

#### Contract Actions

| Status | Name | Duration |
| :---: | --- | ---: |
| ✓ | a transaction to deploy a smart contract, should be reported by the indexer through a transaction query by hash | 7 ms |
| ✓ | a transaction to deploy a smart contract, should be reported by the indexer through a block query by hash | 8 ms |
| ✓ | a transaction to deploy a smart contract, should be reported by the indexer through a contract action query by address | 6 ms |
| ✓ | a transaction to call a smart contract, should be reported by the indexer through a transaction query by hash | 6 ms |
| ✓ | a transaction to call a smart contract, should be reported by the indexer through a block query by hash | 6 ms |
| ✓ | a transaction to call a smart contract, should be reported by the indexer through a contract action query by address | 2 ms |
| ✓ | a transaction to update a smart contract, should be reported by the indexer through a transaction query by hash | 6 ms |
| ✓ | a transaction to update a smart contract, should be reported by the indexer through a block query by hash | 10 ms |
| ✓ | a transaction to update a smart contract, should be reported by the indexer through a contract action query by address | 9 ms |

### Shielded transactions

File name: `tests/e2e/shielded-transactions.test.ts`

#### Shielded Transactions

| Status | Name | Duration |
| :---: | --- | ---: |
| ✓ | a successful shielded transaction transferring 1 Shielded Token between two wallets, should be reported by the indexer through a block query by hash | 10 ms |
| ✓ | a successful shielded transaction transferring 1 Shielded Token between two wallets, should be reported by the indexer through a transaction query by hash | 9 ms |
| ✓ | a successful shielded transaction transferring 1 Shielded Token between two wallets, should stream Zswap events followed by DustSpendProcessed after a shielded transaction | 508 ms |
| ✓ | a successful shielded transaction transferring 1 Shielded Token between two wallets, should increase the zswap Merkle tree end index | 9 ms |
| ✓ | a successful shielded transaction transferring 1 Shielded Token between two wallets, should increase the dust commitment Merkle tree end index | 5 ms |
| ✓ | a confirmed shielded transfer streamed to wallet sessions by viewing key, should stream the transaction to the source viewing key with a matching hash | 4984 ms |
| ✓ | a confirmed shielded transfer streamed to wallet sessions by viewing key, should stream the transaction to the destination viewing key with a matching hash | 2025 ms |
| ✓ | a confirmed shielded transfer streamed to wallet sessions by viewing key, should not stream the transaction to an unrelated viewing key | 20568 ms |

### Unshielded transactions

File name: `tests/e2e/unshielded-transactions.test.ts`

#### Unshielded Transactions

| Status | Name | Duration |
| :---: | --- | ---: |
| ✓ | a successful unshielded transaction transferring 1 STAR between two addresses, should be reported by the indexer through a block query by hash | 10 ms |
| ✓ | a successful unshielded transaction transferring 1 STAR between two addresses, should be reported by the indexer through a transaction query by hash | 7 ms |
| ✓ | a successful unshielded transaction transferring 1 STAR between two addresses, should be reported by the indexer through an unshielded transaction event for the source address | 0 ms |
| ✓ | a successful unshielded transaction transferring 1 STAR between two addresses, should be reported by the indexer through an unshielded transaction event for the destination address | 0 ms |
| ✓ | a successful unshielded transaction transferring 1 STAR between two addresses, should have transferred 1 STAR from the source to the destination address | 9 ms |
| ✓ | a successful unshielded transaction transferring 1 STAR between two addresses, should be reported by the indexer through a progress update event for the source address | 9005 ms |
| ✓ | a successful unshielded transaction transferring 1 STAR between two addresses, should be reported by the indexer through a progress update event for the destination address | 3001 ms |
| ✓ | a successful unshielded transaction transferring 1 STAR between two addresses, should increase the dust commitment Merkle tree end index | 8 ms |
| ✓ | a successful unshielded transaction transferring 1 STAR between two addresses, should deliver dust events in correct sequence after unshielded transaction | 3 ms |

### Wallet subscriptions

File name: `tests/e2e/wallet-subscriptions.test.ts`

#### Wallet Event Subscriptions

| Status | Name | Duration |
| :---: | --- | ---: |
| ✓ | empty wallet scenario, should emit only ProgressUpdate for empty wallet | 2554 ms |
| ✓ | multi-destination transaction scenario, should emit UnshieldedTransaction only for the target wallet (A > B1) | 30091 ms |
| ✓ | multi-destination transaction scenario, should emit UnshieldedTransaction only for the target wallet (A > B2) | 21665 ms |
| ↓ | future coverage, should not duplicate events after resubscription | — |
| ↓ | future coverage, should correctly handle multiple sequential A > B transactions | — |
| ↓ | future coverage, should correctly handle mixed historical and new wallet subscriptions | — |
| ↓ | future coverage, should segregate shielded and unshielded events correctly | — |

### Dust balance

File name: `tests/e2e/toolkit/dust-balance.test.ts`

#### Dust Balance Query Using Toolkit

| Status | Name | Duration |
| :---: | --- | ---: |
| ✓ | a dust balance query with a valid wallet seed, should respond with a dust balance according to the requested schema | 182 ms |

### Key material

File name: `tests/e2e/toolkit/key-material.test.ts`

#### Key Material Derivation Validation

| Status | Name | Duration |
| :---: | --- | ---: |
| ✓ | a midnight shielded address, should show with the expected prefix for the current network ID | 38 ms |
| ✓ | a midnight shielded address, should show with the expected prefix for all network IDs | 163 ms |
| ✓ | a midnight unshielded address, should show with the expected prefix for the current network ID | 37 ms |
| ✓ | a midnight unshielded address, should show with the expected prefix for all network IDs | 126 ms |
| ✓ | a midnight viewing key, should show with the expected prefix for the current network ID | 23 ms |
| ✓ | a midnight viewing key, should show with the expected prefix for all network IDs | 121 ms |

### Show wallet queries

File name: `tests/e2e/toolkit/show-wallet-queries.test.ts`

#### Show Wallet Queries Using Toolkit

| Status | Name | Duration |
| :---: | --- | ---: |
| ✓ | private wallet state query using toolkit, should respond with a private wallet state according to the requested schema | 546 ms |
| ✓ | public wallet state query using toolkit, should respond with a public wallet state according to the requested schema | 464 ms |
