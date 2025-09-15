// This file is part of midnightntwrk/midnight-indexer
// Copyright (C) 2025 Midnight Foundation
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
import { IndexerHttpClient } from '@utils/indexer/http-client';
import type {
  BlockResponse,
  Transaction,
  TransactionOffset,
  TransactionResponse,
  UnshieldedUtxo,
} from '@utils/indexer/indexer-types';

const indexerHttpClient = new IndexerHttpClient();

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
      const offset: TransactionOffset = {
        hash: dataProvider.getKnownTransactionHash(),
      };

      const response: TransactionResponse = await indexerHttpClient.getShieldedTransaction(offset);

      expect(response).toBeSuccess();
      expect(response.data?.transactions).toBeDefined();
      expect(response.data?.transactions).toHaveLength(1);
      expect(response.data?.transactions[0].hash).toBe(offset.hash);
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

      const response = await indexerHttpClient.getShieldedTransaction(transactionOffset);

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
          await indexerHttpClient.getShieldedTransaction(offset);

        expect.soft(response).toBeError();
      }
    });
  });

  describe('a transaction query by identifier', () => {
    /**
     * A transaction query by identifier with a valid & existing identifier returns the expected transaction
     *
     * @given a valid identifier for an existing transaction
     * @when we send a transaction query with that identifier
     * @then Indexer should return the transaction with that identifier
     */
    test('should return the transaction with that identifier, given that transaction exists', async () => {
      const transactionOffset = {
        identifier: dataProvider.getKnownTransactionId(),
      };

      const response: TransactionResponse =
        await indexerHttpClient.getShieldedTransaction(transactionOffset);

      expect(response).toBeSuccess();
      expect(response.data?.transactions).toBeDefined();
      expect(response.data?.transactions).toHaveLength(1);

      // Now that we know there is a transaction, we can check the identifiers (as a transaction could
      // contain more than one identifier)
      const tx = response.data?.transactions[0];
      expect(tx).toBeDefined();
      expect(tx?.identifiers).toBeDefined();
      expect(tx?.identifiers?.length).toBeGreaterThanOrEqual(1);
      expect(tx?.identifiers).toContain(transactionOffset.identifier);
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
        await indexerHttpClient.getShieldedTransaction(transactionOffset);

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

        const response = await indexerHttpClient.getShieldedTransaction(transactionOffset);

        expect.soft(response).toBeError();
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
      const offset: TransactionOffset = {
        hash: '77171f02184423c06e743439273af9e4557c5edf39cdf4125282dba2191e2ad4',
        identifier: '00000000246b12dc2c378d42c8a463db0501b85d93645c4e3fa0e2862590667be36c8b48',
      };

      log.info(
        "Send a transaction query with offset containing both hash and identifier: this shouldn't be allowed",
      );
      let response: TransactionResponse = await indexerHttpClient.getShieldedTransaction(offset);

      expect(response).toBeError();
    });
  });
});

async function getGenesisTransaction(): Promise<Transaction> {
  const blockQueryResponse: BlockResponse = await indexerHttpClient.getBlockByOffset({
    height: 0,
  });
  expect(blockQueryResponse).toBeSuccess();
  expect(blockQueryResponse.data?.block.transactions).toHaveLength(1);

  const genesisTransactionHash = blockQueryResponse.data?.block.transactions[0].hash;
  const transactionQueryResponse = await indexerHttpClient.getShieldedTransaction({
    hash: genesisTransactionHash,
  });

  expect(transactionQueryResponse).toBeSuccess();
  expect(transactionQueryResponse.data?.transactions).toHaveLength(1);

  // NOTE: as Transaction is used to avoid the compiler complaining about the type mismatch
  // this is safe as the data portion of the response can't be undefined, it was validated
  // in the previous expect statements
  const genesisTransaction = transactionQueryResponse.data?.transactions[0] as Transaction;
  expect(genesisTransaction.unshieldedCreatedOutputs).toBeDefined();
  expect(genesisTransaction.unshieldedCreatedOutputs).not.toBeNull();
  expect(genesisTransaction.unshieldedCreatedOutputs?.length).toBeGreaterThanOrEqual(1);

  return genesisTransaction;
}

describe(`genesis transaction`, () => {
  describe(`a transaction query to the genesis block transaction`, async () => {
    let genesisTransaction: Transaction;

    beforeEach(async () => {
      genesisTransaction = await getGenesisTransaction();
    });

    /**
     * Genesis transaction contains utxos related to 4 pre-fund wallets
     *
     * @given the genesis transaction is queried
     * @when we inspect its utxos
     * @then it should return utxos related to 4 pre-fund wallets
     */
    test('should return utxos related to 4 pre-fund wallets', async () => {
      const expectedPreFundWallets = 4;

      // Loop through all the utxos in the genesis transaction and gather all
      // the pre-fund wallet addresses
      const preFundWallets: Set<string> = new Set();
      for (const utxo of genesisTransaction.unshieldedCreatedOutputs!) {
        log.debug(`pre-fund wallet found: ${utxo.owner}`);
        preFundWallets.add(utxo.owner);
      }

      expect(preFundWallets).toHaveLength(expectedPreFundWallets);
    });

    /**
     * Genesis transaction contains utxos with 3 different tokens
     *
     * @given the genesis transaction is queried
     * @when we inspect its utxos
     * @then it should return utxos with 3 different tokens
     */
    test('should return utxos with 3 different tokens', async () => {
      const expectedTokenTypes = 3;

      // Loop through all the utxos in the genesis transaction and gather all
      // available token types
      const tokenTypes: Set<string> = new Set();
      for (const utxo of genesisTransaction.unshieldedCreatedOutputs!) {
        log.debug(`tokenType found: ${utxo.tokenType}`);
        tokenTypes.add(utxo.tokenType);
      }

      expect(tokenTypes).toHaveLength(expectedTokenTypes);
    });

    /**
     * Genesis transaction contains utxos sorted by outputIndex in ascending order
     *
     * @given the genesis transaction is queried
     * @when we inspect its utxos
     * @then the utxos should be sorted by outputIndex in ascending order
     */
    test('should return utxos sorted by outputIndex in ascending order', async () => {
      expect(genesisTransaction.unshieldedCreatedOutputs).not.toBeNull();
      expect(genesisTransaction.unshieldedCreatedOutputs?.length).toBeGreaterThanOrEqual(1);
      const utxos = genesisTransaction.unshieldedCreatedOutputs as UnshieldedUtxo[];
      // Loop through all the utxos in the genesis transaction and check whether the
      // they are sorted by outputIndex in ascending order
      let previousOutputIndex = utxos[0].outputIndex;
      let currentOutputIndex: number;
      for (let i = 1; i < utxos.length; i++) {
        currentOutputIndex = utxos[i].outputIndex;

        // NOTE: We don't need to check that outputIndex values are strictly sequential (e.g., 0, 1, 2, ... N);
        // we only need to verify that they are sorted in ascending order.
        log.debug(
          `previousOutputIndex = ${previousOutputIndex} currentOutputIndex = ${currentOutputIndex}`,
        );
        expect.soft(currentOutputIndex).toBeGreaterThan(previousOutputIndex);
      }
    });
  });
});
