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
import { env } from 'environment/model';
import { MerkleTreeCollapsedUpdateSchema } from '@utils/indexer/graphql/schema';
import { IndexerHttpClient } from '@utils/indexer/http-client';
import type { RegularTransaction } from '@utils/indexer/indexer-types';
import { TestContext } from 'vitest';

const indexerHttpClient = new IndexerHttpClient();

/**
 * Returns the highest `dustGenerationEndIndex` across the regular
 * transactions in the block at the given height, or 0 if the block has
 * no regular transactions exposing the field (e.g. system-only blocks).
 */
async function getMaxDustGenerationEndIndex(blockHeight: number): Promise<number> {
  const response = await indexerHttpClient.getBlockByOffset({ height: blockHeight });
  expect(response).toBeSuccess();
  const transactions = response.data!.block.transactions;
  return transactions.reduce((max, tx) => {
    const regularTx = tx as RegularTransaction;
    return regularTx.dustGenerationEndIndex != null && regularTx.dustGenerationEndIndex > max
      ? regularTx.dustGenerationEndIndex
      : max;
  }, 0);
}

/**
 * Probes the indexer to find the smallest `endIndex` that lies just
 * beyond the currently-cached dust generation tree boundary. Replaces
 * a hard-coded magic value (e.g. `999_999_999`) that can drift into a
 * legitimate range on long-running envs. Strategy:
 *   - request a range; if the indexer returns a successful payload,
 *     `endIndex` is within the cache;
 *   - if the indexer returns a GraphQL error, `endIndex` is beyond.
 * Exponential probe upward to bracket the boundary, then binary-search
 * inside the bracket. Bounded to ~60 indexer calls in the worst case
 * with the cap below; on QANET this completes in well under a second.
 */
async function findFirstBeyondRangeEndIndex(): Promise<number> {
  const cap = 2_000_000_000; // safely below GraphQL Int32 max
  let lo = 1;
  let hi = 1;
  while (hi < cap) {
    const response = await indexerHttpClient.getDustGenerationMerkleTreeUpdate(0, hi);
    if (response.errors && response.errors.length > 0) break;
    lo = hi;
    hi = Math.min(hi * 2, cap);
  }
  if (hi === cap) {
    throw new Error(
      `failed to find a beyond-range endIndex below ${cap}: indexer cache appears unexpectedly large for the test env`,
    );
  }
  while (lo + 1 < hi) {
    const mid = Math.floor((lo + hi) / 2);
    const response = await indexerHttpClient.getDustGenerationMerkleTreeUpdate(0, mid);
    if (response.errors && response.errors.length > 0) {
      hi = mid;
    } else {
      lo = mid;
    }
  }
  return hi;
}

// Dust generation registrations require a Cardano-side mapping which has no
// counterpart in the `undeployed` environment. Skip the whole surface there;
// re-enable once #1152 lands local Cardano test-data provisioning.
describe.skipIf(env.isUndeployedEnv())('dust generation merkle tree update queries', () => {
  describe('a collapsed update query with valid index range', () => {
    /**
     * A dust generation update query with a valid index range returns the expected result
     *
     * @given the chain has indexed blocks with dust generation state
     * @when we query for a dust generation update with startIndex=0 and endIndex=1
     * @then Indexer should return a valid collapsed update
     */
    test('should return a collapsed update for a valid index range', async (ctx: TestContext) => {
      ctx.task!.meta.custom = {
        labels: ['Query', 'Dust', 'GenerationMerkleTree', 'CollapsedUpdate'],
      };

      log.debug('Requesting dust generation merkle tree update with startIndex=0, endIndex=1');
      const response = await indexerHttpClient.getDustGenerationMerkleTreeUpdate(0, 1);

      expect(response).toBeSuccess();
      expect(response.data?.dustGenerationMerkleTreeUpdate).toBeDefined();

      const collapsedUpdate = response.data!.dustGenerationMerkleTreeUpdate;
      expect(collapsedUpdate.startIndex).toBe(0);
      expect(collapsedUpdate.endIndex).toBe(1);
      expect(collapsedUpdate.update).toBeDefined();
      expect(collapsedUpdate.protocolVersion).toBeDefined();
    });

    /**
     * A dust generation update query responds with the expected schema
     *
     * @given the chain has indexed blocks with dust generation state
     * @when we query for a dust generation update with a valid index range
     * @then the response should match the expected schema
     */
    test('should respond with a collapsed update according to the expected schema', async (ctx: TestContext) => {
      ctx.task!.meta.custom = {
        labels: ['Query', 'Dust', 'GenerationMerkleTree', 'CollapsedUpdate', 'SchemaValidation'],
      };

      log.debug('Requesting dust generation merkle tree update with startIndex=0, endIndex=1');
      const response = await indexerHttpClient.getDustGenerationMerkleTreeUpdate(0, 1);

      expect(response).toBeSuccess();
      expect(response.data?.dustGenerationMerkleTreeUpdate).toBeDefined();

      log.debug('Validating collapsed update schema');
      const parsed = MerkleTreeCollapsedUpdateSchema.safeParse(
        response.data!.dustGenerationMerkleTreeUpdate,
      );
      expect(
        parsed.success,
        `Collapsed update schema validation failed ${JSON.stringify(parsed.error, null, 2)}`,
      ).toBe(true);
    });

    /**
     * A dust generation update query covering the full genesis dust range returns a valid result
     *
     * @given the genesis block has indexed dust generation state
     * @when we query for a dust generation update covering the full range from genesis
     * @then Indexer should return a valid collapsed update spanning the entire range
     */
    test('should return a collapsed update for the full genesis dust range', async (ctx: TestContext) => {
      ctx.task!.meta.custom = {
        labels: ['Query', 'Dust', 'GenerationMerkleTree', 'CollapsedUpdate', 'FullRange'],
      };

      const maxEndIndex = await getMaxDustGenerationEndIndex(0);

      log.debug(`Highest dustGenerationEndIndex from genesis: ${maxEndIndex}`);
      expect(maxEndIndex).toBeGreaterThan(0);

      // dustGenerationEndIndex is exclusive, collapsed update endIndex is inclusive
      const endIndex = maxEndIndex - 1;

      log.debug(`Requesting dust generation update with startIndex=0, endIndex=${endIndex}`);
      const response = await indexerHttpClient.getDustGenerationMerkleTreeUpdate(0, endIndex);

      expect(response).toBeSuccess();
      expect(response.data?.dustGenerationMerkleTreeUpdate).toBeDefined();

      const collapsedUpdate = response.data!.dustGenerationMerkleTreeUpdate;
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
        labels: ['Query', 'Dust', 'GenerationMerkleTree', 'CollapsedUpdate', 'EdgeCase'],
      };

      log.debug('Requesting dust generation merkle tree update with startIndex=0, endIndex=0');
      const response = await indexerHttpClient.getDustGenerationMerkleTreeUpdate(0, 0);

      expect(response).toBeSuccess();
      expect(response.data?.dustGenerationMerkleTreeUpdate).toBeDefined();

      const collapsedUpdate = response.data!.dustGenerationMerkleTreeUpdate;
      expect(collapsedUpdate.startIndex).toBe(0);
      expect(collapsedUpdate.endIndex).toBe(0);
      expect(collapsedUpdate.update).toBeDefined();
      expect(collapsedUpdate.protocolVersion).toBeDefined();
    });
  });

  describe('a collapsed update query idempotency', () => {
    /**
     * Two identical dust generation update queries should return the same result
     *
     * @given we query the same index range twice
     * @when the chain head has not changed between calls
     * @then both responses should be identical
     */
    test('should return identical results for the same query parameters', async (ctx: TestContext) => {
      ctx.task!.meta.custom = {
        labels: ['Query', 'Dust', 'GenerationMerkleTree', 'CollapsedUpdate', 'Idempotency'],
      };

      log.debug(
        'Requesting dust generation merkle tree update twice with startIndex=0, endIndex=1',
      );
      const response1 = await indexerHttpClient.getDustGenerationMerkleTreeUpdate(0, 1);
      const response2 = await indexerHttpClient.getDustGenerationMerkleTreeUpdate(0, 1);

      expect(response1).toBeSuccess();
      expect(response2).toBeSuccess();

      const update1 = response1.data!.dustGenerationMerkleTreeUpdate;
      const update2 = response2.data!.dustGenerationMerkleTreeUpdate;

      expect(update1.startIndex).toBe(update2.startIndex);
      expect(update1.endIndex).toBe(update2.endIndex);
      expect(update1.update).toBe(update2.update);
      expect(update1.protocolVersion).toBe(update2.protocolVersion);
    });
  });

  describe('a collapsed update query with invalid index range', () => {
    /**
     * A dust generation update query where startIndex > endIndex should return an error
     *
     * @given we use an invalid index range where startIndex > endIndex
     * @when we query for a dust generation update
     * @then Indexer should respond with an error about invalid start_index and/or end_index
     */
    test('should return an error when startIndex is greater than endIndex', async (ctx: TestContext) => {
      ctx.task!.meta.custom = {
        labels: ['Query', 'Dust', 'GenerationMerkleTree', 'CollapsedUpdate', 'Negative'],
      };

      log.debug('Requesting dust generation merkle tree update with startIndex=10, endIndex=5');
      const response = await indexerHttpClient.getDustGenerationMerkleTreeUpdate(10, 5);

      expect(response).toBeError();
      expect(response.errors![0].message).toContain('invalid start_index and/or end_index');
    });

    /**
     * A dust generation update query with negative indices should return a parse error
     *
     * @given we use negative indices
     * @when we query for a dust generation update
     * @then Indexer should respond with an error about invalid number parsing
     */
    test('should return an error when indices are negative', async (ctx: TestContext) => {
      ctx.task!.meta.custom = {
        labels: ['Query', 'Dust', 'GenerationMerkleTree', 'CollapsedUpdate', 'Negative'],
      };

      log.debug('Requesting dust generation merkle tree update with startIndex=-1, endIndex=1');
      const response = await indexerHttpClient.getDustGenerationMerkleTreeUpdate(-1, 1);

      expect(response).toBeError();
      expect(response.errors![0].message).toContain('Invalid number');
    });

    /**
     * A dust generation update query where endIndex exceeds the indexed range should return an error
     *
     * @given we use an endIndex strictly beyond the indexer's currently-indexed
     *        dust generation tree boundary (derived from the chain tip rather
     *        than hard-coded, so the test stays correct on long-running envs)
     * @when we query for a dust generation update
     * @then Indexer should respond with an error about invalid start_index and/or end_index
     */
    test('should return an error when endIndex is beyond the indexed range', async (ctx: TestContext) => {
      ctx.task!.meta.custom = {
        labels: ['Query', 'Dust', 'GenerationMerkleTree', 'CollapsedUpdate', 'Negative'],
      };

      const beyondRangeEndIndex = await findFirstBeyondRangeEndIndex();

      log.debug(
        `Requesting dust generation merkle tree update with startIndex=0, endIndex=${beyondRangeEndIndex} (probed first-beyond-range boundary)`,
      );
      const response = await indexerHttpClient.getDustGenerationMerkleTreeUpdate(
        0,
        beyondRangeEndIndex,
      );

      expect(response).toBeError();
      expect(response.errors![0].message).toContain('invalid start_index and/or end_index');
    });
  });
});
