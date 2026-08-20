# Indexer Test Execution Report

## Run Details

| Field | Value |
| --- | --- |
| Component Under Test | Indexer / Node |
| Indexer Version | 4.3.3 |
| Node Version | 1.0.0 |
| Test Suites | smoke, integration |
| Tests | Indexer GraphQL API and integration with Node |
| Environment | qanet (green instance) |
| Date | Friday 5th Jun 2026 |
| QA Contact | Giuseppe Salvatore (giuseppe.salvatore@shielded.io) |

## Summary

| Suite | Test Files | Passed | Failed | Skipped | Total | Duration | Run at |
| --- | --- | ---: | ---: | ---: | ---: | --- | --- |
| smoke | 3 passed, 1 skipped (4) | 18 | 0 | 1 | 19 | 1.53s | 10:07 PM BST |
| integration | 19 passed (19) | 168 | 0 | 2 | 170 | 29.04s | 10:07 PM BST |
| **Total** | | **186** | **0** | **3** | **189** | | |

> Legend: ✓ passed · ✗ failed · ↓ skipped.

## Smoke results

### Graphql healthchecks

File name: `tests/smoke/graphql-healthchecks.test.ts`

#### Graphql Health Checks

| Status | Name | Duration |
| :---: | --- | ---: |
| ✓ | an introspection message sent to the websocket channel, should return the supported graphql schema | 560 ms |
| ✓ | an introspection message sent to the websocket channel, should return an error, given the depth of the query is > 15 | 547 ms |
| ✓ | an introspection request sent to the http channel, should return the supported graphql schema | 129 ms |
| ✓ | an introspection request sent to the http channel, should return an error, given the depth of the query is > 15 | 69 ms |

### Graphql schema change

File name: `tests/smoke/graphql-schema-change.test.ts`

#### GraphQL Schema Stability Check

| Status | Name | Duration |
| :---: | --- | ---: |
| ↓ | should not change the schema unexpectedly | — |

### Graphql schema deprecations

File name: `tests/smoke/graphql-schema-deprecations.test.ts`

#### Graphql Schema Deprecations

| Status | Name | Duration |
| :---: | --- | ---: |
| ✓ | RegularTransaction.merkleTreeRoot, should be marked deprecated with reason "Use zswapMerkleTreeRoot instead" | 95 ms |
| ✓ | RegularTransaction.startIndex, should be marked deprecated with reason "Use zswapStartIndex instead" | 67 ms |
| ✓ | RegularTransaction.endIndex, should be marked deprecated with reason "Use zswapEndIndex instead" | 22 ms |
| ✓ | RelevantTransaction.collapsedMerkleTree, should be marked deprecated with reason "Use zswapCollapsedUpdate instead" | 20 ms |
| ✓ | ShieldedTransactionsProgress.highestEndIndex, should be marked deprecated with reason "Use highestZswapEndIndex instead" | 24 ms |
| ✓ | ShieldedTransactionsProgress.highestCheckedEndIndex, should be marked deprecated with reason "Use highestCheckedZswapEndIndex instead" | 24 ms |
| ✓ | ShieldedTransactionsProgress.highestRelevantEndIndex, should be marked deprecated with reason "Use highestRelevantZswapEndIndex instead" | 23 ms |
| ✓ | RegularTransaction.fees, should be marked deprecated with reason "Use fee instead" | 21 ms |
| ✓ | TransactionFees.estimatedFees, should be marked deprecated with reason "Use paidFees instead" | 21 ms |

### Service healthchecks

File name: `tests/smoke/service-healthchecks.test.ts`

#### Service Health Checks

| Status | Name | Duration |
| :---: | --- | ---: |
| ✓ | a request to the /ready endpoint, should return a 200 status code OK | 91 ms |
| ✓ | a request to an unrecognised path /api/v3/__regression_unknown_path, should not 308 to a version-double-prefixed Location | 53 ms |
| ✓ | a request to an unrecognised path /api/v3/__regression_unknown_path, should terminate when redirects are followed | 22 ms |
| ✓ | a request to an unrecognised path /api/v4/__regression_unknown_path, should not 308 to a version-double-prefixed Location | 20 ms |
| ✓ | a request to an unrecognised path /api/v4/__regression_unknown_path, should terminate when redirects are followed | 20 ms |

## Integration results

### Block queries

File name: `tests/integration/basic/queries/block-queries.test.ts`

#### Block Queries

| Status | Name | Duration |
| :---: | --- | ---: |
| ✓ | a block query without parameters, should return the latest known block | 155 ms |
| ✓ | a block query without parameters, should respond with a block according to the requested schema | 33 ms |
| ✓ | a block query by hash, should return the block with that hash, given that block exists | 55 ms |
| ✓ | a block query by hash, should return blocks according to the requested schema | 68 ms |
| ✓ | a block query by hash, should return a null block, given a block with that hash doesn't exist | 19 ms |
| ✓ | a block query by hash, should return an error, when the hash is invalid (malformed) | 87 ms |
| ✓ | a block query by height, should return the block with that height, given a valid height | 43 ms |
| ✓ | a block query by height, should return a blocks according to the requested schema | 44 ms |
| ✓ | a block query by height, should return the genesis block, given height=0 is requested | 483 ms |
| ✓ | a block query by height, should return an empty body answer, given that block height requested is the maximum available height | 21 ms |
| ✓ | a block query by height, should return an error, given an invalid height | 51 ms |
| ✓ | a block query by height and hash, should return an error, as only one parameter at a time can be used | 64 ms |

#### Genesis Block

| Status | Name | Duration |
| :---: | --- | ---: |
| ✓ | a block query to the genesis block, should contain transactions with pre-fund wallet utxos | 547 ms |
| ✓ | a block query to the genesis block, should contain utxos related to exactly 4 pre-fund wallets | 146 ms |
| ✓ | a block query to the genesis block, should contain utxos with exactly 1 token type | 118 ms |
| ✓ | a block query to the genesis block, should contain utxos sorted by outputIndex in ascending order | 98 ms |
| ✓ | a block query to the genesis block, should contain valid index ranges on regular transactions | 115 ms |

### Contract queries

File name: `tests/integration/basic/queries/contract-queries.test.ts`

#### Contract Queries

| Status | Name | Duration |
| :---: | --- | ---: |
| ✓ | a contract query by address, should return success as they are valid contract addresses | 220 ms |
| ✓ | a contract query by address, should return the most recent action for a contract with multiple actions | 54 ms |
| ✓ | a contract query by address, should return null when contract with that address does not exist | 29 ms |
| ✓ | a contract query by address, should return an error for malformed addresses | 288 ms |
| ✓ | a contract query by address and offset, should return the correct action using exact block hash where it was included | 26 ms |
| ✓ | a contract query by address and offset, should return the latest state using a future block hash | 43 ms |
| ✓ | a contract query by address and offset, should respond with a contract action according to the expected schema | 27 ms |
| ✓ | a contract query by address and offset, should return the correct action using exact block height where it was included | 29 ms |
| ✓ | a contract query by address and offset, should return the latest state using a future block height | 35 ms |
| ✓ | a contract query by address and offset, should return the most recent contract action for that address before the specified block | 56 ms |
| ✓ | a contract query by address and offset, should return null when contract with valid address and valid offset does not exist | 19 ms |
| ✓ | a contract query by address and offset, should return null when contract with valid address and non-existing hash does not exist | 25 ms |
| ✓ | a contract query by address and offset, should return error when contract with invalid address and valid hash | 14 ms |
| ✓ | a contract query by address and offset, should return error when contract with invalid address and non-existing hash | 18 ms |
| ✓ | a contract query by address and offset, should return error when contract with valid address and invalid hash | 15 ms |
| ✓ | a contract query by address and offset, should return error when contract with invalid address and invalid hash | 81 ms |
| ✓ | a contract query by address and offset, should return null when contract with valid address and valid height does not exist | 25 ms |
| ✓ | a contract query by address and offset, should return null when contract with valid address and non-existing height does not exist | 30 ms |
| ✓ | a contract query by address and offset, should return error when contract with invalid address and valid height | 15 ms |
| ✓ | a contract query by address and offset, should return error when contract with invalid address and invalid height | 50 ms |
| ✓ | a contract query by address and offset, should return error for negative height | 16 ms |
| ✓ | a contract query by address and offset, should return error for non-integer height | 15 ms |
| ✓ | a contract query by address and offset, should return error for extremely large height | 16 ms |
| ✓ | a contract query by address and offset, should return null when using a block hash from before the action existed | 474 ms |

### Dust commitment update queries

File name: `tests/integration/basic/queries/dust-commitment-update-queries.test.ts`

#### Dust Commitment Merkle Tree Update Queries

| Status | Name | Duration |
| :---: | --- | ---: |
| ✓ | a collapsed update query with valid index range, should return a collapsed update for a valid index range | 102 ms |
| ✓ | a collapsed update query with valid index range, should respond with a collapsed update according to the expected schema | 106 ms |
| ✓ | a collapsed update query with valid index range, should return a collapsed update for the full genesis dust range | 614 ms |
| ✓ | a collapsed update query with equal start and end indices, should return a valid update when startIndex equals endIndex | 19 ms |
| ✓ | a collapsed update query idempotency, should return identical results for the same query parameters | 51 ms |
| ✓ | a collapsed update query with invalid index range, should return an error when startIndex is greater than endIndex | 24 ms |
| ✓ | a collapsed update query with invalid index range, should return an error when indices are negative | 18 ms |
| ✓ | a collapsed update query with invalid index range, should return an error when endIndex is beyond the indexed range | 26 ms |

### Dust generation update queries

File name: `tests/integration/basic/queries/dust-generation-update-queries.test.ts`

#### Dust Generation Merkle Tree Update Queries

| Status | Name | Duration |
| :---: | --- | ---: |
| ✓ | a collapsed update query with valid index range, should return a collapsed update for a valid index range | 97 ms |
| ✓ | a collapsed update query with valid index range, should respond with a collapsed update according to the expected schema | 64 ms |
| ✓ | a collapsed update query with valid index range, should return a collapsed update for the full genesis dust range | 184 ms |
| ✓ | a collapsed update query with equal start and end indices, should return a valid update when startIndex equals endIndex | 18 ms |
| ✓ | a collapsed update query idempotency, should return identical results for the same query parameters | 47 ms |
| ✓ | a collapsed update query with invalid index range, should return an error when startIndex is greater than endIndex | 18 ms |
| ✓ | a collapsed update query with invalid index range, should return an error when indices are negative | 18 ms |
| ✓ | a collapsed update query with invalid index range, should return an error when endIndex is beyond the indexed range | 890 ms |

### Dust generations queries

File name: `tests/integration/basic/queries/dust-generations-queries.test.ts`

#### Dust Generations Queries

| Status | Name | Duration |
| :---: | --- | ---: |
| ✓ | a dust generations query with a valid Cardano reward address, should return dust generations for a registered address | 104 ms |
| ✓ | a dust generations query with a valid Cardano reward address, should respond with dust generations according to the expected schema | 80 ms |
| ✓ | a dust generations query with a valid Cardano reward address, should return registrations with valid fields for a registered address | 27 ms |
| ✓ | a dust generations query with multiple addresses, should return dust generations for multiple addresses | 25 ms |
| ✓ | a dust generations query with non-registered address, should return empty registrations for a non-registered address | 28 ms |
| ✓ | a dust generations query with invalid input, should return an empty result for an empty addresses array | 19 ms |
| ✓ | a dust generations query with invalid input, should return an error for a malformed address | 23 ms |
| ✓ | a dustGenerations query with a multi-UTXO stake key (#926), should aggregate nightBalance across all backing cNIGHT UTXOs | 54 ms |
| ✓ | backwards compatibility with dustGenerationStatus, should return consistent data from both endpoints for a registered address | 74 ms |

### Dust queries

File name: `tests/integration/basic/queries/dust-queries.test.ts`

#### Dust Generation Status Queries

| Status | Name | Duration |
| :---: | --- | ---: |
| ✓ | a dust generation status query with a valid Cardano reward address, should respond with a dust generation status response according to the requested schema | 91 ms |
| ✓ | a dust generation status query with a valid Cardano reward address, should report registered status for a registered Cardano reward address | 76 ms |
| ✓ | a dust generation status query with a valid Cardano reward address, should give the DUST destination address for the expected network when Cardano reward address is registered | 24 ms |
| ✓ | a dust generation status query with a valid Cardano reward address, should correctly indicate registration status for a non-registered key | 23 ms |
| ✓ | a dust generation status query with a valid Cardano reward address, should indicate zero generation for registered address without cNIGHT balance | 25 ms |
| ✓ | a dust generation status query with a valid Cardano reward address, should report the correct value of max capacity for registered address with cNIGHT | 41 ms |
| ✓ | a dust generation status query with a valid Cardano reward address, should correctly indicate deregistered status for a previously registered Cardano reward address | 22 ms |
| ✓ | a dust generation status query with multiple valid Cardano reward addresses, should return statuses for multiple Cardano reward addresses in order | 80 ms |
| ✓ | a dust generation status query with multiple valid Cardano reward addresses, should respond with dust generation statuses according to the requested schema for multiple addresses | 35 ms |
| ✓ | a dust generation status query with multiple valid Cardano reward addresses, should return an error given the number of addresses is greater than 10 | 19 ms |
| ✓ | a dust generation status query with malformed reward addresses, should return an error when the address is in plain hex string format | 17 ms |
| ✓ | a dust generation status query with empty list of reward addresses, should return an empty list of dust generation statuses | 17 ms |
| ✓ | a dust generation status query with a Cardano payment address, should return an error as only Cardano reward addresses are supported | 15 ms |
| ✓ | a dust generation status query with a Cardano reward address not meant for this network, should return an error reporting the target network mismatch | 19 ms |
| ✓ | a dust generation status query with duplicate reward addresses, should handle duplicate Cardano reward addresses appropriately | 33 ms |

### Transaction queries

File name: `tests/integration/basic/queries/transaction-queries.test.ts`

#### Transaction Queries

| Status | Name | Duration |
| :---: | --- | ---: |
| ✓ | a transaction query by hash, should return the transaction with that hash, given that transaction exists | 1289 ms |
| ✓ | a transaction query by hash, should return an empty transaction list, given a transaction with that hash doesn't exist | 23 ms |
| ✓ | a transaction query by hash, should return an error, given a hash is invalid (malformed) | 80 ms |
| ✓ | a transaction query by hash, should return an error when called with an empty offset object | 16 ms |
| ✓ | a transaction query by identifier, should return the transaction with that identifier, given that transaction exists | 1363 ms |
| ✓ | a transaction query by identifier, should return an empty list of transactions, given a transaction with that identifier doesn't exist | 65 ms |
| ✓ | a transaction query by identifier, should return an error, given an invalid identifier | 48 ms |
| ✓ | a transaction query by hash and identifier, should return an error, as only one parameter at a time can be used | 16 ms |

#### Genesis Transactions

| Status | Name | Duration |
| :---: | --- | ---: |
| ✓ | transaction queries to the genesis block transactions, should return utxos related to 4 pre-fund wallets | 1304 ms |
| ✓ | transaction queries to the genesis block transactions, should return utxos with 1 token type | 793 ms |
| ✓ | transaction queries to the genesis block transactions, should return utxos sorted by outputIndex in ascending order | 845 ms |
| ✓ | transaction queries to the genesis block transactions, should return system transactions when queried by hash | 1012 ms |
| ✓ | transaction queries to the genesis block transactions, should not contain RegularTransaction-specific fields on system transactions | 796 ms |
| ✓ | schema validation, should respond with full transaction data according to the expected schema | 6 ms |
| ✓ | schema validation, should respond with system transactions according to the expected schema | 0 ms |
| ✓ | schema validation, should contain both regular and system transactions | 0 ms |
| ✓ | schema validation, should respond with regular transactions according to the expected schema | 2 ms |
| ✓ | schema validation, should respond with nested ledger events and unshielded outputs according to the expected schema | 1 ms |

### Zswap collapsed update queries

File name: `tests/integration/basic/queries/zswap-collapsed-update-queries.test.ts`

#### Zswap Merkle Tree Collapsed Update Queries

| Status | Name | Duration |
| :---: | --- | ---: |
| ✓ | a collapsed update query with valid index range, should return a collapsed update for a valid index range | 87 ms |
| ✓ | a collapsed update query with valid index range, should respond with a collapsed update according to the expected schema | 69 ms |
| ✓ | a collapsed update query with valid index range, should return a collapsed update for the full genesis zswap range | 171 ms |
| ✓ | a collapsed update query with equal start and end indices, should return a valid update when startIndex equals endIndex | 23 ms |
| ✓ | a collapsed update query idempotency, should return identical results for the same query parameters | 42 ms |
| ✓ | a collapsed update query with invalid index range, should return an error when startIndex is greater than endIndex | 17 ms |
| ✓ | a collapsed update query with invalid index range, should return an error when indices are negative | 15 ms |
| ✓ | a collapsed update query with invalid index range, should return an error when endIndex is beyond the indexed range | 22 ms |

### Block subscriptions

File name: `tests/integration/basic/subscriptions/block-subscriptions.test.ts`

#### Block Subscriptions

| Status | Name | Duration |
| :---: | --- | ---: |
| ✓ | a subscription to block updates without parameters, should stream blocks starting from the latest block | 4513 ms |
| ✓ | a subscription to block updates without parameters, should stream blocks adhering to the expected schema | 653 ms |
| ✓ | a subscription to block updates by hash, should stream blocks starting from the block with that hash, given that hash exists | 1227 ms |
| ✓ | a subscription to block updates by hash, should return an error message, given that hash doesn't exist | 597 ms |
| ✓ | a subscription to block updates by hash, should return an error message, given that hash is invalid | 594 ms |
| ✓ | a subscription to block updates by height, should stream blocks from the block with that height, given that height exists | 682 ms |
| ✓ | a subscription to block updates by height, should return an error message, given that height is higher than the latest block height | 620 ms |
| ✓ | a subscription to block updates by height, should return an error message, given that height is invalid | 592 ms |
| ✓ | a subscription to block updates by height and hash, should return an error message, as only one parameter at a time can be used | 585 ms |
| ↓ | regular transaction fees, should report ledger paidFees and estimatedFees on regular transactions | — |

### Contract subscriptions

File name: `tests/integration/basic/subscriptions/contract-subscriptions.test.ts`

#### Contract Action Subscriptions

| Status | Name | Duration |
| :---: | --- | ---: |
| ✓ | a subscription to contract action updates without parameters, should stream contract actions from the latest available block | 612 ms |
| ✓ | a subscription to contract action updates without parameters, should stream contract actions adhering to the expected schema | 598 ms |
| ✓ | a subscription to contract action updates with block hash offset, should stream historical and new contract actions from a specific block hash | 588 ms |

### Dust generations subscriptions

File name: `tests/integration/basic/subscriptions/dust-generations-subscriptions.test.ts`

#### Dust Generations Subscription

| Status | Name | Duration |
| :---: | --- | ---: |
| ✓ | streaming dust generation entries, should stream dust generation events for a valid dust address in bech32m format | 1425 ms |
| ✓ | subscription error handling, should return an error for an invalid dust address | 548 ms |
| ✓ | subscription error handling, should return an error for a valid address that is meant for another networkid | 694 ms |
| ✓ | subscription error handling, should return an error for a valid dust address passed in hex format | 546 ms |
| ✓ | transactionHash on dust generation events (#1114), first item transactionHash resolves via transactions(offset) | 1021 ms |

### Dust ledger subscriptions

File name: `tests/integration/basic/subscriptions/dust-ledger-subscriptions.test.ts`

#### Dust Ledger Event Subscriptions

| Status | Name | Duration |
| :---: | --- | ---: |
| ✓ | a subscription to dust ledger events without offset (default replay), streams events in ledger order | 597 ms |
| ✓ | subscription with explicit offset, streams events in ledger order starting from the specified ID | 598 ms |
| ✓ | subscription with explicit offset, validates historical dust events against schema | 588 ms |
| ✓ | subscription error handling, should return an error for unknown field | 543 ms |
| ✓ | subscription error handling, rejects negative offset ID with an error | 544 ms |

### Dust nullifier subscriptions

File name: `tests/integration/basic/subscriptions/dust-nullifier-subscriptions.test.ts`

#### Dust Nullifier Transactions Subscription

| Status | Name | Duration |
| :---: | --- | ---: |
| ✓ | streaming dust nullifier transactions with block range, should stream transactions within a block range and complete | 648 ms |
| ✓ | subscription error handling, should return an error for empty nullifier prefixes | 547 ms |
| ✓ | subscription error handling, should return an error when fromBlock is greater than toBlock | 548 ms |
| ✓ | transactionHash on dust nullifier events (#1114), first event transactionHash resolves via transactions(offset) | 687 ms |

### Shielded nullifier subscriptions

File name: `tests/integration/basic/subscriptions/shielded-nullifier-subscriptions.test.ts`

#### Shielded Nullifier Transactions Subscription

| Status | Name | Duration |
| :---: | --- | ---: |
| ✓ | streaming shielded nullifier transactions with block range, should stream transactions within a block range and complete | 644 ms |
| ✓ | subscription error handling, should return an error for empty nullifier prefixes | 547 ms |
| ✓ | subscription error handling, should return an error for an empty-string nullifier prefix element | 548 ms |
| ✓ | subscription error handling, should return an error when fromBlock is greater than toBlock | 547 ms |
| ↓ | transactionHash on shielded nullifier events (#1114), first event transactionHash resolves via transactions(offset) | 631 ms |

### Shielded transaction subscriptions

File name: `tests/integration/basic/subscriptions/shielded-transaction-subscriptions.test.ts`

#### Shielded Transaction Subscriptions

| Status | Name | Duration |
| :---: | --- | ---: |
| ✓ | opening a session with viewing key, should return a session ID, given a valid viewing key | 617 ms |
| ✓ | opening a session with viewing key, should return an error, given an unsupported hex format viewing key | 548 ms |
| ✓ | opening a session with viewing key, should return an error, given an invalid viewing key | 543 ms |
| ✓ | opening a session with viewing key, should return an error, given a valid viewing key meant for a different network | 908 ms |
| ✓ | closing a session with session ID, should terminate the session successfully, given a valid session ID | 676 ms |
| ✓ | closing a session with session ID, should return an error, given an invalid session ID | 548 ms |
| ✓ | a subscription to wallet updates providing viewing key only, should stream wallet events starting from the beginning, given there are relevant transactions | 2611 ms |
| ✓ | a subscription to wallet updates providing viewing key only, should stream shielded transaction events adhering to the expected schema | 3603 ms |
| ✓ | a subscription to wallet updates providing viewing key only, should be able to use highestZswapEndIndex from progress event in collapsed update query | 716 ms |
| ✓ | a subscription to wallet updates providing viewing key only, should reject shieldedTransactions subscription when using expired session ID | 711 ms |

### Subscription quotas

File name: `tests/integration/basic/subscriptions/subscription-quotas.test.ts`

#### Subscription Quotas (HAL-03 / SSE-196)

| Status | Name | Duration |
| :---: | --- | ---: |
| ✓ | per-connection concurrent subscription cap, should reject a subscription beyond the per-connection cap | 1562 ms |
| ✓ | per-connection concurrent subscription cap, should free a slot when an active subscription is closed | 2553 ms |

### Unshielded transaction subscriptions

File name: `tests/integration/basic/subscriptions/unshielded-transaction-subscriptions.test.ts`

#### Unshielded Transaction Subscriptions

| Status | Name | Duration |
| :---: | --- | ---: |
| ✓ | a subscription to unshielded transaction events by address, should stream unshielded transaction events related to that address, given that address has transactions | 498 ms |
| ✓ | a subscription to unshielded transaction events by address, should stream unshielded transaction events up to highest transaction id | 5046 ms |
| ✓ | a subscription to unshielded transaction events by address, should stream unshielded transaction events that adhere to the expected schema | 130 ms |
| ✓ | a subscription to unshielded transaction events by address, should only return a transaction progress message with highest transaction = 0, given that address does not have transactions | 5049 ms |
| ✓ | a subscription to unshielded transaction events by address, should return an error message, given the address provided is in hex format | 14 ms |
| ✓ | a subscription to unshielded transaction events by address, should return an error message, given the address provided is for another network | 558 ms |
| ✓ | a subscription to unshielded transaction events by address and transaction id, should return a stream of transactions containing that address, starting from transaction id = 0 | 5562 ms |
| ✓ | a subscription to unshielded transaction events by address and transaction id, should return an error message, given the transaction id provided is negative | 70 ms |
| ✓ | a subscription to unshielded transaction events by address and transaction id, should only return a transaction progress message without streaming transactions, given that the transaction id provided is bigger number | 5060 ms |
| ✓ | a subscription to unshielded transaction events by address and transaction id, should start a transaction stream from the given transaction id | 5296 ms |
| ✓ | a subscription to unshielded transaction events by address and transaction id, should return an error message, given the address is provided in hex format | 61 ms |

### Wallet connect options

File name: `tests/integration/basic/subscriptions/wallet-connect-options.test.ts`

#### Wallet Connect Options (StartIndex)

| Status | Name | Duration |
| :---: | --- | ---: |
| ✓ | opening a session with startIndex, should accept startIndex = 0 (no-op equivalent of unset options) | 612 ms |
| ✓ | opening a session with startIndex, should skip historical scan when startIndex equals the current tip | 1216 ms |
| ✓ | opening a session with startIndex, should accept startIndex past the current tip (fast-forward) | 1222 ms |

### Zswap events subscriptions

File name: `tests/integration/basic/subscriptions/zswap-events-subscriptions.test.ts`

#### Zswap Ledger Event Subscriptions

| Status | Name | Duration |
| :---: | --- | ---: |
| ✓ | a subscription to zswap ledger events without offset (default replay), streams events in ledger order | 590 ms |
| ✓ | subscription with explicit offset, streams events starting from the specified ID | 634 ms |
| ✓ | subscription with explicit offset, validates historical zswap events against schema | 578 ms |
| ✓ | subscription error handling, should return an error for unknown field | 548 ms |
| ✓ | subscription error handling, rejects negative offset ID with an error | 548 ms |
