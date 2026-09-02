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
import { MerkleTreeCollapsedUpdateSchema } from '@utils/indexer/graphql/schema';
import { IndexerHttpClient } from '@utils/indexer/http-client';
import type { RegularTransaction } from '@utils/indexer/indexer-types';
import { TestContext } from 'vitest';

const indexerHttpClient = new IndexerHttpClient();

describe('dust commitment merkle tree update queries', () => {
  describe('a collapsed update query with valid index range', () => {
    /**
     * A dust commitment update query with a valid index range returns the expected result
     *
     * @given the chain has indexed blocks with dust commitment state
     * @when we query for a dust commitment update with startIndex=0 and endIndex=1
     * @then Indexer should return a valid collapsed update
     */
    test('should return a collapsed update for a valid index range', async (ctx: TestContext) => {
      ctx.task!.meta.custom = {
        labels: ['Query', 'Dust', 'CommitmentMerkleTree', 'CollapsedUpdate'],
      };

      log.debug('Requesting dust commitment merkle tree update with startIndex=0, endIndex=1');
      const response = await indexerHttpClient.getDustCommitmentMerkleTreeUpdate(0, 1);

      expect(response).toBeSuccess();
      expect(response.data?.dustCommitmentMerkleTreeUpdate).toBeDefined();

      const collapsedUpdate = response.data!.dustCommitmentMerkleTreeUpdate;
      expect(collapsedUpdate.startIndex).toBe(0);
      expect(collapsedUpdate.endIndex).toBe(1);
      expect(collapsedUpdate.update).toBeDefined();
      expect(collapsedUpdate.protocolVersion).toBeDefined();
    });

    /**
     * A dust commitment update query responds with the expected schema
     *
     * @given the chain has indexed blocks with dust commitment state
     * @when we query for a dust commitment update with a valid index range
     * @then the response should match the expected schema
     */
    test('should respond with a collapsed update according to the expected schema', async (ctx: TestContext) => {
      ctx.task!.meta.custom = {
        labels: ['Query', 'Dust', 'CommitmentMerkleTree', 'CollapsedUpdate', 'SchemaValidation'],
      };

      log.debug('Requesting dust commitment merkle tree update with startIndex=0, endIndex=1');
      const response = await indexerHttpClient.getDustCommitmentMerkleTreeUpdate(0, 1);

      expect(response).toBeSuccess();
      expect(response.data?.dustCommitmentMerkleTreeUpdate).toBeDefined();

      log.debug('Validating collapsed update schema');
      const parsed = MerkleTreeCollapsedUpdateSchema.safeParse(
        response.data!.dustCommitmentMerkleTreeUpdate,
      );
      expect(
        parsed.success,
        `Collapsed update schema validation failed ${JSON.stringify(parsed.error, null, 2)}`,
      ).toBe(true);
    });

    /**
     * A dust commitment update query covering the full genesis dust range returns a valid result
     *
     * @given the genesis block has indexed dust commitment state
     * @when we query for a dust commitment update covering the full range from genesis
     * @then Indexer should return a valid collapsed update spanning the entire range
     */
    test('should return a collapsed update for the full genesis dust range', async (ctx: TestContext) => {
      ctx.task!.meta.custom = {
        labels: ['Query', 'Dust', 'CommitmentMerkleTree', 'CollapsedUpdate', 'FullRange'],
      };

      // Get the highest dustCommitmentEndIndex from genesis block transactions
      const genesisResponse = await indexerHttpClient.getBlockByOffset({ height: 0 });
      expect(genesisResponse).toBeSuccess();

      const transactions = genesisResponse.data!.block.transactions;
      const maxEndIndex = transactions.reduce((max, tx) => {
        const regularTx = tx as RegularTransaction;
        return regularTx.dustCommitmentEndIndex != null && regularTx.dustCommitmentEndIndex > max
          ? regularTx.dustCommitmentEndIndex
          : max;
      }, 0);

      log.debug(`Highest dustCommitmentEndIndex from genesis: ${maxEndIndex}`);
      expect(maxEndIndex).toBeGreaterThan(0);

      // dustCommitmentEndIndex is exclusive, collapsed update endIndex is inclusive
      const endIndex = maxEndIndex - 1;

      log.debug(`Requesting dust commitment update with startIndex=0, endIndex=${endIndex}`);
      const response = await indexerHttpClient.getDustCommitmentMerkleTreeUpdate(0, endIndex);

      expect(response).toBeSuccess();
      expect(response.data?.dustCommitmentMerkleTreeUpdate).toBeDefined();

      const collapsedUpdate = response.data!.dustCommitmentMerkleTreeUpdate;
      expect(collapsedUpdate.startIndex).toBe(0);
      expect(collapsedUpdate.endIndex).toBe(endIndex);
      expect(collapsedUpdate.update).toBeDefined();
      expect(collapsedUpdate.protocolVersion).toBeDefined();
    });
  });

  describe('a collapsed update query with equal start and end indices', () => {
    /**
     * A collapsed update query where startIndex === endIndex returns a valid trivial update
     *
     * @given we use startIndex equal to endIndex
     * @when we query for a collapsed update
     * @then Indexer should return a valid collapsed update with matching indices
     */
    test('should return a valid update when startIndex equals endIndex', async (ctx: TestContext) => {
      ctx.task!.meta.custom = {
        labels: ['Query', 'Dust', 'CommitmentMerkleTree', 'CollapsedUpdate', 'EdgeCase'],
      };

      log.debug('Requesting dust commitment merkle tree update with startIndex=0, endIndex=0');
      const response = await indexerHttpClient.getDustCommitmentMerkleTreeUpdate(0, 0);

      expect(response).toBeSuccess();
      expect(response.data?.dustCommitmentMerkleTreeUpdate).toBeDefined();

      const collapsedUpdate = response.data!.dustCommitmentMerkleTreeUpdate;
      expect(collapsedUpdate.startIndex).toBe(0);
      expect(collapsedUpdate.endIndex).toBe(0);
      expect(collapsedUpdate.update).toBeDefined();
      expect(collapsedUpdate.protocolVersion).toBeDefined();
    });
  });

  describe('a collapsed update query idempotency', () => {
    /**
     * Two identical dust commitment update queries should return the same result
     *
     * @given we query the same index range twice
     * @when the chain head has not changed between calls
     * @then both responses should be identical
     */
    test('should return identical results for the same query parameters', async (ctx: TestContext) => {
      ctx.task!.meta.custom = {
        labels: ['Query', 'Dust', 'CommitmentMerkleTree', 'CollapsedUpdate', 'Idempotency'],
      };

      log.debug(
        'Requesting dust commitment merkle tree update twice with startIndex=0, endIndex=1',
      );
      const response1 = await indexerHttpClient.getDustCommitmentMerkleTreeUpdate(0, 1);
      const response2 = await indexerHttpClient.getDustCommitmentMerkleTreeUpdate(0, 1);

      expect(response1).toBeSuccess();
      expect(response2).toBeSuccess();

      const update1 = response1.data!.dustCommitmentMerkleTreeUpdate;
      const update2 = response2.data!.dustCommitmentMerkleTreeUpdate;

      expect(update1.startIndex).toBe(update2.startIndex);
      expect(update1.endIndex).toBe(update2.endIndex);
      expect(update1.update).toBe(update2.update);
      expect(update1.protocolVersion).toBe(update2.protocolVersion);
    });
  });

  describe('a collapsed update query with invalid index range', () => {
    /**
     * A dust commitment update query where startIndex > endIndex should return an error
     *
     * @given we use an invalid index range where startIndex > endIndex
     * @when we query for a dust commitment update
     * @then Indexer should respond with an error about invalid start_index and/or end_index
     */
    test('should return an error when startIndex is greater than endIndex', async (ctx: TestContext) => {
      ctx.task!.meta.custom = {
        labels: ['Query', 'Dust', 'CommitmentMerkleTree', 'CollapsedUpdate', 'Negative'],
      };

      log.debug('Requesting dust commitment merkle tree update with startIndex=10, endIndex=5');
      const response = await indexerHttpClient.getDustCommitmentMerkleTreeUpdate(10, 5);

      expect(response).toBeError();
      expect(response.errors![0].message).toContain('invalid start_index and/or end_index');
    });

    /**
     * A dust commitment update query with negative indices should return a parse error
     *
     * @given we use negative indices
     * @when we query for a dust commitment update
     * @then Indexer should respond with an error about invalid number parsing
     */
    test('should return an error when indices are negative', async (ctx: TestContext) => {
      ctx.task!.meta.custom = {
        labels: ['Query', 'Dust', 'CommitmentMerkleTree', 'CollapsedUpdate', 'Negative'],
      };

      log.debug('Requesting dust commitment merkle tree update with startIndex=-1, endIndex=1');
      const response = await indexerHttpClient.getDustCommitmentMerkleTreeUpdate(-1, 1);

      expect(response).toBeError();
      expect(response.errors![0].message).toContain('Invalid number');
    });

    /**
     * A dust commitment update query where endIndex exceeds the indexed range should return an error
     *
     * @given we use an endIndex far beyond the current indexed range
     * @when we query for a dust commitment update
     * @then Indexer should respond with an error about invalid start_index and/or end_index
     */
    test('should return an error when endIndex is beyond the indexed range', async (ctx: TestContext) => {
      ctx.task!.meta.custom = {
        labels: ['Query', 'Dust', 'CommitmentMerkleTree', 'CollapsedUpdate', 'Negative'],
      };

      log.debug(
        'Requesting dust commitment merkle tree update with startIndex=0, endIndex=999999999',
      );
      const response = await indexerHttpClient.getDustCommitmentMerkleTreeUpdate(0, 999999999);

      expect(response).toBeError();
      expect(response.errors![0].message).toContain('invalid start_index and/or end_index');
    });
  });
});
