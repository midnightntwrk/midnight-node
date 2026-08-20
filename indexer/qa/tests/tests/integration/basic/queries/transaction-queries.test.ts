// This file is part of midnightntwrk/midnight-indexer
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

import log from '@utils/logging/logger';
import '@utils/logging/test-logging-hooks';
import dataProvider from '@utils/testdata-provider';
import { GraphQLError } from 'graphql/error/GraphQLError';
import { IndexerHttpClient } from '@utils/indexer/http-client';
import type {
  BlockResponse,
  RegularTransaction,
  SystemTransaction,
  Transaction,
  TransactionOffset,
  TransactionResponse,
  UnshieldedUtxo,
} from '@utils/indexer/indexer-types';
import {
  FullTransactionSchema,
  RegularTransactionSchema,
  SystemTransactionSchema,
  ZswapLedgerEventSchema,
  DustLedgerEventSchema,
  UnshieldedUtxoSchema,
} from '@utils/indexer/graphql/schema';

const indexerHttpClient = new IndexerHttpClient();

// Helper functions
async function getGenesisBlock(): Promise<BlockResponse> {
  const blockResponse = await indexerHttpClient.getBlockByOffset({ height: 0 });
  expect(blockResponse).toBeSuccess();
  expect(blockResponse.data?.block.transactions.length).toBeGreaterThanOrEqual(1);
  return blockResponse;
}

async function getGenesisTransactionsByHash(): Promise<Transaction[]> {
  const blockResponse = await getGenesisBlock();
  const transactionHashes = blockResponse.data!.block.transactions.map(
    (transaction) => transaction.hash,
  );

  const genesisTransactions: Transaction[] = [];

  for (const hash of transactionHashes) {
    const response = await indexerHttpClient.getTransactionByOffset({ hash });
    expect(response).toBeSuccess();
    expect(Array.isArray(response.data?.transactions)).toBe(true);
    expect(response.data?.transactions?.length).toBeGreaterThanOrEqual(1);

    if (response.data?.transactions[0]) {
      genesisTransactions.push(response.data.transactions[0]);
    }
  }

  expect(genesisTransactions.length).toBeGreaterThanOrEqual(1);
  return genesisTransactions;
}

function getRegularTransactions(transactions: Transaction[]): RegularTransaction[] {
  return transactions.filter(
    (tx) => tx.__typename === 'RegularTransaction',
  ) as RegularTransaction[];
}

function getSystemTransactions(transactions: Transaction[]): SystemTransaction[] {
  return transactions.filter((tx) => tx.__typename === 'SystemTransaction') as SystemTransaction[];
}

function extractUtxos(transactions: Transaction[]): UnshieldedUtxo[] {
  const regularTxs = getRegularTransactions(transactions);
  return regularTxs.flatMap((tx) => tx.unshieldedCreatedOutputs || []);
}

describe('transaction queries', () => {
  describe('a transaction query by hash', () => {
    /**
     * A transaction query by hash with a valid & existing hash returns the expected transaction
     *
     * @given an hash for an existing transaction
     * @when we send a transaction query with that hash
     * @then Indexer should return the transaction with that hash
     */
    test(`should return the transaction with that hash, given that transaction exists`, async () => {
      const genesisTransactions = await getGenesisTransactionsByHash();
      expect(Array.isArray(genesisTransactions)).toBe(true);
      expect(genesisTransactions.length).toBeGreaterThanOrEqual(1);

      for (const tx of genesisTransactions) {
        expect.soft(tx.hash).toBeDefined();
        expect.soft(tx.__typename).toBeDefined();
      }
    });

    /**
     * A transaction query by hash with a valid & non-existing hash returns an empty list
     *
     * @given an hash for a non-existent transaction
     * @when we send a transaction query with that hash
     * @then Indexer should return an empty transaction list
     */
    test(`should return an empty transaction list, given a transaction with that hash doesn't exist`, async () => {
      const transactionOffset = {
        hash: '0000000000000000000000000000000000000000000000000000000000000000',
      };

      const response = await indexerHttpClient.getTransactionByOffset(transactionOffset);

      expect(response).toBeSuccess();
      expect(response.data?.transactions).toBeDefined();
      expect(response.data?.transactions).toHaveLength(0);
    });

    /**
     * A transaction query by hash returns an error if hash is invalid (malformed)
     *
     * @given we fabricate an invalid hashes (malformed)
     * @when we send a transaction query by hash using them
     * @then Indexer should return an error
     */
    test('should return an error, given a hash is invalid (malformed)', async () => {
      const fabricatedMalformedHashes = dataProvider.getFabricatedMalformedHashes();

      for (const targetHash of fabricatedMalformedHashes) {
        const offset: TransactionOffset = {
          hash: targetHash,
        };

        log.info(`Send a transaction query with an hash longer than expected: ${targetHash}`);
        const response: TransactionResponse =
          await indexerHttpClient.getTransactionByOffset(offset);

        expect.soft(response).toBeError();
        const messages = response.errors?.map((e) => e.message) ?? [];
        expect.soft(messages[0]).toContain('invalid transaction hash: cannot');
      }
    });

    /**
     * A transaction query with an empty offset object should fail validation
     *
     * @given an empty offset object
     * @when we send a transaction query without specifying hash or height
     * @then the Indexer should return an error response
     */
    test('should return an error when called with an empty offset object', async () => {
      const offset: TransactionOffset = {};
      const response = await indexerHttpClient.getTransactionByOffset(offset);

      expect.soft(response).toBeError();
      const errorMessages = response.errors?.map((e: GraphQLError) => e.message) ?? [];
      expect
        .soft(errorMessages[0])
        .toContain(
          'Invalid value for argument "offset", Oneof input objects requires have exactly one field',
        );
    });
  });

  describe('a transaction query by identifier', () => {
    /**
     * A transaction query by identifier with a valid & existing identifier returns the expected transaction
     * We use one of the identifiers from the regular transactions in the genesis block.
     *
     * @given a valid identifier for an existing transaction
     * @when we send a transaction query with that identifier
     * @then Indexer should return the transaction with that identifier
     */
    test('should return the transaction with that identifier, given that transaction exists', async () => {
      const blockResponse = await getGenesisBlock();
      const transactions = blockResponse.data!.block.transactions;

      const regularTransactions = getRegularTransactions(transactions);
      const identifiers = regularTransactions
        .map((tx) => tx.identifiers?.[0])
        .filter((id): id is string => !!id);

      expect.soft(identifiers.length).toBeGreaterThanOrEqual(1);

      for (const identifier of identifiers) {
        const transactionQueryResponse = await indexerHttpClient.getTransactionByOffset({
          identifier,
        });

        expect.soft(transactionQueryResponse).toBeSuccess();
        expect.soft(transactionQueryResponse.data?.transactions).toHaveLength(1);

        const transaction = transactionQueryResponse.data?.transactions?.[0];
        expect.soft(transaction?.__typename).toBe('RegularTransaction');

        const regularTransaction = transaction as RegularTransaction;
        expect.soft(regularTransaction.identifiers).toBeDefined();
        expect.soft(regularTransaction.identifiers?.length).toBeGreaterThanOrEqual(1);
        expect.soft(regularTransaction.identifiers).toContain(identifier);
      }
    });

    /**
     * A transaction query by indentifier with a valid & non-existent identifier returns an empty list
     *
     * @given a valid identifier for a non-existent transaction
     * @when we send a transaction query with that identifier
     * @then Indexer should return an empty list of transactions
     */
    test(`should return an empty list of transactions, given a transaction with that identifier doesn't exist`, async () => {
      const transactionOffset = {
        identifier: '0000000000000000000000000000000000000000000000000000000000000000',
      };

      const response: TransactionResponse =
        await indexerHttpClient.getTransactionByOffset(transactionOffset);

      expect(response).toBeSuccess();
      expect(response.data!.transactions).toBeDefined();
      expect(response.data!.transactions).toHaveLength(0);
    });

    /**
     * Transaction queries by indentifier with invalid identifiers return an error
     *
     * @given an invalid identifier
     * @when we send a transaction query with that identifier
     * @then Indexer should return an error
     */
    test(`should return an error, given an invalid identifier`, async () => {
      const invalidIdentifiers = dataProvider.getFabricatedMalformedIdentifiers();

      for (const invalidIdentifier of invalidIdentifiers) {
        const transactionOffset = {
          identifier: invalidIdentifier,
        };

        const response = await indexerHttpClient.getTransactionByOffset(transactionOffset);

        expect.soft(response).toBeError();
        const messages = response.errors?.map((e) => e.message) ?? [];
        expect.soft(messages[0]).toContain('invalid transaction identifier: cannot');
      }
    });
  });

  describe('a transaction query by hash and identifier', () => {
    /**
     * A transaction query with both hash and identifier returns an error
     *
     * @given both hash and identifier are specified in the offset
     * @when we send a transaction query with both parameters
     * @then Indexer should return an error
     */
    test('should return an error, as only one parameter at a time can be used', async () => {
      // Note here we are building an offset object with random validly formed hash and identifier
      // The fact these are random doesn't matter because the indexer should reject the query with an
      // error before trying to see if a transaction with that hash and identifier exists.
      const offset: TransactionOffset = {
        hash: '77171f02184423c06e743439273af9e4557c5edf39cdf4125282dba2191e2ad4',
        identifier: '00000000246b12dc2c378d42c8a463db0501b85d93645c4e3fa0e2862590667be36c8b48',
      };

      log.info(
        "Send a transaction query with offset containing both hash and identifier: this shouldn't be allowed",
      );
      let response: TransactionResponse = await indexerHttpClient.getTransactionByOffset(offset);

      expect(response).toBeError();
      const errorMessages = response.errors?.map((e: GraphQLError) => e.message) ?? [];
      expect
        .soft(errorMessages[0])
        .toContain(
          'Invalid value for argument "offset", Oneof input objects requires have exactly one field',
        );
    });
  });
});

describe(`genesis transactions`, () => {
  describe(`transaction queries to the genesis block transactions`, async () => {
    let genesisTransactions: Transaction[];

    beforeEach(async () => {
      genesisTransactions = await getGenesisTransactionsByHash();
    });

    /**
     * Genesis regular transactions contain utxos related to 4 pre-fund wallets
     *
     * @given the genesis transactions are collected
     * @when we inspect their utxos
     * @then some of the regular transactions should contain utxos related to 4 pre-fund wallets
     */
    test('should return utxos related to 4 pre-fund wallets', async () => {
      const expectedPreFundWallets = 4;

      // Loop through all the utxos in the genesis transaction and gather all
      // the pre-fund wallet addresses
      const utxos = extractUtxos(genesisTransactions);
      const preFundWallets = new Set(utxos.map((u) => u.owner));

      for (const owner of preFundWallets) {
        log.debug(`pre-fund wallet found: ${owner}`);
      }

      expect(preFundWallets).toHaveLength(expectedPreFundWallets);
    });

    /**
     * Genesis transactions contain utxos with 1 token type
     *
     * @given the genesis transactions are queried
     * @when we inspect their utxos
     * @then some of the regular transactions should contain utxos with 1 token type
     */
    test('should return utxos with 1 token type', async () => {
      const expectedTokenTypes = 1;

      // Loop through all the utxos in the genesis transactions and gather all
      // available token types
      const utxos = extractUtxos(genesisTransactions);
      const tokenTypes = new Set(utxos.map((u) => u.tokenType));

      expect(tokenTypes).toHaveLength(expectedTokenTypes);
    });

    /**
     * Genesis transactions contain utxos sorted by outputIndex in ascending order
     *
     * @given the genesis transactions are queried
     * @when we inspect their utxos
     * @then the utxos should be sorted by outputIndex in ascending order
     */
    test('should return utxos sorted by outputIndex in ascending order', async () => {
      // Loop through all the utxos in each of the genesis transactions and check whether the
      // utxos are sorted by outputIndex in ascending order
      const utxos = extractUtxos(genesisTransactions);

      // Verify the entire combined UTXO list is sorted by outputIndex
      for (let i = 1; i < utxos.length; i++) {
        expect(utxos[i].outputIndex).toBeGreaterThanOrEqual(utxos[i - 1].outputIndex);
      }
    });

    /**
     * System transactions should be individually queryable by their hash,
     * the same way regular transactions are.
     *
     * @given a system transaction hash from the genesis block
     * @when we query the transaction by hash
     * @then the indexer should return the system transaction with matching hash and type
     */
    test('should return system transactions when queried by hash', async () => {
      const systemTxs = getSystemTransactions(genesisTransactions);
      expect(systemTxs.length).toBeGreaterThanOrEqual(1);

      for (const tx of systemTxs) {
        const response: TransactionResponse = await indexerHttpClient.getTransactionByOffset({
          hash: tx.hash!,
        });

        expect(response).toBeSuccess();
        expect(response.data?.transactions.length).toBeGreaterThanOrEqual(1);

        const queriedTx = response.data!.transactions[0];
        expect(queriedTx.__typename).toBe('SystemTransaction');
        expect(queriedTx.hash).toBe(tx.hash);

        log.debug(`Successfully queried system tx by hash: ${tx.hash}`);
      }
    });

    /**
     * SystemTransactions should not have RegularTransaction-specific fields like
     * zswapMerkleTreeRoot, identifiers, fees, or transactionResult.
     *
     * @given system transactions are fetched from the genesis block
     * @when we inspect their fields
     * @then RegularTransaction-specific fields should not be present
     */
    test('should not contain RegularTransaction-specific fields on system transactions', async () => {
      const systemTxs = getSystemTransactions(genesisTransactions);
      expect(systemTxs.length).toBeGreaterThanOrEqual(1);

      for (const tx of systemTxs) {
        const txAny = tx as unknown as Record<string, unknown>;
        expect(txAny['zswapMerkleTreeRoot']).toBeUndefined();
        expect(txAny['identifiers']).toBeUndefined();
        expect(txAny['fees']).toBeUndefined();
        expect(txAny['transactionResult']).toBeUndefined();
      }
    });
  });

  describe('schema validation', () => {
    let genesisTransactions: Transaction[];

    beforeAll(async () => {
      genesisTransactions = await getGenesisTransactionsByHash();
    });

    /**
     * Validates that all genesis transactions comply with the expected structure.
     *
     * @given the genesis transactions are fetched from the indexer
     * @when each transaction is validated against the FullTransactionSchema
     * @then all transactions should successfully pass schema validation
     */
    test('should respond with full transaction data according to the expected schema', async () => {
      expect(genesisTransactions.length).toBeGreaterThan(0);

      for (const tx of genesisTransactions) {
        const result = FullTransactionSchema.safeParse(tx);

        expect(
          result.success,
          `FulTransaction Schema validation failed: ${JSON.stringify(result.error?.format?.(), null, 2)}`,
        ).toBe(true);
      }
    });

    /**
     * Ensures that all SystemTransactions contain the required system-specific fields.
     *
     * @given the genesis transactions are fetched from the indexer
     * @when transactions with __typename = 'SystemTransaction' are validated
     * @then each SystemTransaction should match the SystemTransactionSchema
     */
    test('should respond with system transactions according to the expected schema', async () => {
      const systemTxs = genesisTransactions.filter((tx) => tx.__typename === 'SystemTransaction');

      for (const tx of systemTxs) {
        const result = SystemTransactionSchema.safeParse(tx);
        expect(
          result.success,
          `SystemTransaction schema validation failed for tx ${tx.hash}: ${JSON.stringify(result.error?.format?.(), null, 2)}`,
        ).toBe(true);
      }
    });

    /**
     * The genesis block should contain both RegularTransaction and SystemTransaction types,
     * verifying that SystemTransactionApplied events are captured by the indexer
     * and system transactions appear alongside regular transactions in GraphQL queries.
     *
     * @given the genesis block has been indexed
     * @when we query the genesis block transactions
     * @then both RegularTransaction and SystemTransaction types should be present
     */
    test('should contain both regular and system transactions', async () => {
      const typenames = new Set(genesisTransactions.map((tx) => tx.__typename));

      log.debug(`Transaction types found in genesis: ${[...typenames].join(', ')}`);
      expect(typenames).toContain('RegularTransaction');
      expect(typenames).toContain('SystemTransaction');
    });

    /**
     * Ensures that all RegularTransactions contain the expected fields.
     *
     * @given the genesis transactions are fetched from the indexer
     * @when transactions with __typename = 'RegularTransaction' are validated
     * @then each RegularTransaction should match the RegularTransactionSchema
     */
    test('should respond with regular transactions according to the expected schema', async () => {
      const regularTxs = getRegularTransactions(genesisTransactions);

      for (const tx of regularTxs) {
        const result = RegularTransactionSchema.safeParse(tx);
        expect(
          result.success,
          `RegularTransaction schema validation failed for tx ${tx.hash}: ${JSON.stringify(result.error?.format?.(), null, 2)}`,
        ).toBe(true);
      }
    });

    /**
     * Ensures that all nested structures within RegularTransactions
     * conform to their respective schemas.
     *
     * @given the RegularTransactions are fetched from the genesis block
     * @when validating zswapLedgerEvents, dustLedgerEvents, and unshieldedCreatedOutputs
     * @then each nested entity should conform to its expected schema
     */
    test('should respond with nested ledger events and unshielded outputs according to the expected schema', async () => {
      const regularTxs = getRegularTransactions(genesisTransactions);

      // zswapLedgerEvents
      regularTxs
        .filter((tx) => tx.zswapLedgerEvents?.length)
        .forEach((tx) => {
          log.debug(
            `Validating ${tx.zswapLedgerEvents!.length} zswapLedgerEvents for tx ${tx.hash}`,
          );
          tx.zswapLedgerEvents!.forEach((event) => {
            const result = ZswapLedgerEventSchema.safeParse(event);
            expect(
              result.success,
              `ZswapLedgerEvent schema validation failed for tx ${tx.hash}: ${JSON.stringify(result.error?.format?.(), null, 2)}`,
            ).toBe(true);
          });
        });

      // dustLedgerEvents
      regularTxs
        .filter((tx) => tx.dustLedgerEvents?.length)
        .forEach((tx) => {
          log.debug(`Validating ${tx.dustLedgerEvents!.length} dustLedgerEvents for tx ${tx.hash}`);
          tx.dustLedgerEvents!.forEach((event) => {
            const result = DustLedgerEventSchema.safeParse(event);
            expect(
              result.success,
              `DustLedgerEvent schema validation failed for tx ${tx.hash}: ${JSON.stringify(result.error?.format?.(), null, 2)}`,
            ).toBe(true);
          });
        });

      // unshieldedCreatedOutputs
      regularTxs
        .filter((tx) => tx.unshieldedCreatedOutputs?.length)
        .forEach((tx) => {
          log.debug(
            `Validating ${tx.unshieldedCreatedOutputs!.length} unshieldedCreatedOutputs for tx ${tx.hash}`,
          );
          tx.unshieldedCreatedOutputs!.forEach((output) => {
            const result = UnshieldedUtxoSchema.safeParse(output);
            expect(
              result.success,
              `UnshieldedUtxo schema validation failed for tx ${tx.hash}: ${JSON.stringify(result.error?.format?.(), null, 2)}`,
            ).toBe(true);
          });
        });
    });
  });
});
