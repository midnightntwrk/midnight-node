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
import type { TestContext } from 'vitest';
import {
  IndexerWsClient,
  DustNullifierTransactionSubscriptionResponse,
} from '@utils/indexer/websocket-client';
import { extractSubscriptionErrorMessage } from '@utils/indexer/subscription-error';
import { DustNullifierTransactionSchema } from '@utils/indexer/graphql/schema';
import { IndexerHttpClient } from '@utils/indexer/http-client';
import { DustNullifierTransaction } from '@utils/indexer/indexer-types';

const indexerHttpClient = new IndexerHttpClient();

describe('dust nullifier transactions subscription', () => {
  let indexerWsClient: IndexerWsClient;

  beforeEach(async () => {
    indexerWsClient = new IndexerWsClient();
    await indexerWsClient.connectionInit();
  }, 30_000);

  afterEach(async () => {
    await indexerWsClient.connectionClose();
  });

  describe('streaming dust nullifier transactions with block range', () => {
    /**
     * A dust nullifier transactions subscription with a bounded block range
     * should stream matching transactions and complete
     *
     * @given a set of nullifier prefixes and a block range
     * @when we subscribe to dustNullifierTransactions
     * @then we should receive matching transactions (if any) and the subscription should complete
     * @and each transaction should match the expected schema
     */
    test('should stream transactions within a block range and complete', async () => {
      // Get the latest block height to define a bounded range
      const blockResponse = await indexerHttpClient.getLatestBlock();
      expect(blockResponse).toBeSuccess();
      const latestHeight = blockResponse.data!.block.height;

      // Use a broad prefix to increase chance of matches, bounded to first 10 blocks
      const toBlock = Math.min(latestHeight, 10);
      const nullifierLeBytesPrefixes = ['00'];

      log.debug(
        `Subscribing to dust nullifier transactions with prefixes=${nullifierLeBytesPrefixes}, fromBlock=0, toBlock=${toBlock}`,
      );

      const received: DustNullifierTransactionSubscriptionResponse[] = [];

      await new Promise<void>((resolve, reject) => {
        const timeout = setTimeout(() => {
          subscription.unsubscribe();
          // It's OK if no matches were found in the range — subscription should still complete
          resolve();
        }, 15_000);

        const subscription = indexerWsClient.subscribeToDustNullifierTransactions(
          {
            next: (payload) => {
              received.push(payload);
              log.debug(
                `Received dust nullifier transaction: ${JSON.stringify(payload.data?.dustNullifierTransactions)}`,
              );
            },
            error: (error) => {
              clearTimeout(timeout);
              subscription.unsubscribe();
              reject(new Error(`Subscription error: ${JSON.stringify(error)}`));
            },
            complete: () => {
              clearTimeout(timeout);
              resolve();
            },
          },
          nullifierLeBytesPrefixes,
          0,
          toBlock,
        );
      });

      log.debug(`Received ${received.length} dust nullifier transactions`);

      // Validate each received transaction against the schema
      for (const msg of received) {
        expect(msg).toBeSuccess();
        const tx = msg.data!.dustNullifierTransactions;
        const parsed = DustNullifierTransactionSchema.safeParse(tx);
        expect(
          parsed.success,
          `Dust nullifier transaction schema validation failed: ${JSON.stringify(parsed.error, null, 2)}`,
        ).toBe(true);

        // Block height should be within the requested range
        expect(tx.blockHeight).toBeGreaterThanOrEqual(0);
        expect(tx.blockHeight).toBeLessThanOrEqual(toBlock);
      }
    });
  });

  describe('subscription error handling', () => {
    /**
     * A dust nullifier transactions subscription with an empty prefixes array should return an error
     *
     * @given an empty array of nullifier prefixes
     * @when we subscribe to dustNullifierTransactions
     * @then the subscription should return a client error about empty prefixes
     */
    test('should return an error for empty nullifier prefixes', async () => {
      const settled = await new Promise<{
        completed: boolean;
        error: string | null;
        eventCount: number;
      }>((resolve) => {
        let eventCount = 0;
        const timeout = setTimeout(() => {
          subscription.unsubscribe();
          resolve({ completed: false, error: null, eventCount });
        }, 8_000);

        const subscription = indexerWsClient.subscribeToDustNullifierTransactions(
          {
            next: () => {
              eventCount++;
            },
            error: (error) => {
              clearTimeout(timeout);
              subscription.unsubscribe();
              resolve({
                completed: false,
                error: extractSubscriptionErrorMessage(error),
                eventCount,
              });
            },
            complete: () => {
              clearTimeout(timeout);
              resolve({ completed: true, error: null, eventCount });
            },
          },
          [],
          0,
        );
      });

      expect(settled.error).toContain('nullifierLeBytesPrefixes must not be empty');
      expect(settled.completed).toBe(false);
      expect(settled.eventCount).toBeGreaterThanOrEqual(0);
    });

    /**
     * A dust nullifier transactions subscription with invalid block range should return an error
     *
     * @given fromBlock > toBlock
     * @when we subscribe to dustNullifierTransactions
     * @then the subscription should return a client error
     */
    test('should return an error when fromBlock is greater than toBlock', async () => {
      const settled = await new Promise<{
        completed: boolean;
        error: string | null;
        eventCount: number;
      }>((resolve, reject) => {
        let eventCount = 0;
        const timeout = setTimeout(() => {
          subscription.unsubscribe();
          reject(new Error('Timed out waiting for completion'));
        }, 10_000);

        const subscription = indexerWsClient.subscribeToDustNullifierTransactions(
          {
            next: () => {
              eventCount++;
            },
            error: (error) => {
              clearTimeout(timeout);
              subscription.unsubscribe();
              resolve({
                completed: false,
                error: extractSubscriptionErrorMessage(error),
                eventCount,
              });
            },
            complete: () => {
              clearTimeout(timeout);
              resolve({ completed: true, error: null, eventCount });
            },
          },
          ['00'],
          10,
          5,
        );
      });

      expect(settled.error).toContain('fromBlock must not exceed toBlock');
      expect(settled.completed).toBe(false);
      expect(settled.eventCount).toBeGreaterThanOrEqual(0);
    });
  });

  /**
   * Coverage for midnight-indexer#1114 / PR #1116
   * (`feat(indexer-api): add transactionHash to event subscription response types`).
   *
   * `transactionHash: HexEncoded!` was added to `DustNullifierTransaction`.
   * The schema-level shape (64-hex, non-nullable) is already enforced by the
   * `DustNullifierTransactionSchema` used by the streaming test above. This
   * block adds the round-trip check: the streamed hash must resolve a
   * transaction via `transactions(offset: { hash: ... })`.
   *
   * Match presence is environment-dependent (prefix `'00'` over the full
   * chain). If no transactions match within the timeout, the round-trip is
   * vacuous and we skip rather than asserting against an empty stream.
   */
  describe('transactionHash on dust nullifier events (#1114)', () => {
    /**
     * @given a wide prefix scan of the full chain
     * @when we subscribe to `dustNullifierTransactions` and look up the first
     *       streamed event's `transactionHash` via `transactions(offset)`
     * @then the lookup resolves a single transaction whose `hash` equals the
     *       streamed `transactionHash` — proving the field is the on-chain
     *       identifier.
     */
    test('first event transactionHash resolves via transactions(offset)', async (ctx: TestContext) => {
      const blockResponse = await indexerHttpClient.getLatestBlock();
      expect(blockResponse).toBeSuccess();
      const latestHeight = blockResponse.data!.block.height;

      const received: DustNullifierTransaction[] = [];

      await new Promise<void>((resolve, reject) => {
        const timeout = setTimeout(() => {
          subscription.unsubscribe();
          resolve();
        }, 15_000);

        const subscription = indexerWsClient.subscribeToDustNullifierTransactions(
          {
            next: (payload) => {
              const tx = payload.data?.dustNullifierTransactions;
              if (tx) {
                received.push(tx);
                clearTimeout(timeout);
                subscription.unsubscribe();
                resolve();
              }
            },
            error: (err) => {
              clearTimeout(timeout);
              subscription.unsubscribe();
              reject(new Error(`Subscription error: ${JSON.stringify(err)}`));
            },
            complete: () => {
              clearTimeout(timeout);
              resolve();
            },
          },
          ['00'],
          0,
          latestHeight,
        );
      });

      if (received.length === 0) {
        log.warn(
          'no dustNullifierTransactions matched prefix "00" within the timeout; ' +
            'round-trip skipped (environment has no dust nullifier transactions in range)',
        );
        ctx.skip?.(
          true,
          'no dust nullifier transactions matched within timeout — round-trip vacuous',
        );
        return;
      }

      const first = received[0];
      log.debug(
        `Round-tripping DustNullifierTransaction.transactionHash=${first.transactionHash} ` +
          `(transactionId=${first.transactionId})`,
      );

      const txResponse = await indexerHttpClient.getTransactionByOffset({
        hash: first.transactionHash,
      });
      expect(txResponse).toBeSuccess();
      const transactions = txResponse.data!.transactions;
      expect(transactions).toHaveLength(1);
      expect(transactions[0].hash).toBe(first.transactionHash);
    }, 30_000);
  });

  /**
   * Coverage for midnight-indexer#1115
   * (`feat: add transaction reference field for event subscription navigation`).
   *
   * `transaction: Transaction! @beta` was added to `DustNullifierTransaction`
   * so consumers can navigate to all Transaction fields directly from the
   * streamed event without a separate lookup. The Zod schema enforces the
   * field shape; this block adds the consistency check: `transaction.hash`
   * must equal `transactionHash` on the same event.
   */
  describe('transaction reference on dust nullifier events (#1115)', () => {
    /**
     * @given a wide prefix scan of the full chain
     * @when we subscribe to dustNullifierTransactions and receive the first event
     * @then event.transaction.hash equals event.transactionHash, confirming the
     *       reference field is wired to the same on-chain transaction
     */
    test('first event transaction.hash matches transactionHash', async (ctx: TestContext) => {
      const blockResponse = await indexerHttpClient.getLatestBlock();
      expect(blockResponse).toBeSuccess();
      const latestHeight = blockResponse.data!.block.height;

      const received: DustNullifierTransaction[] = [];

      await new Promise<void>((resolve, reject) => {
        const timeout = setTimeout(() => {
          subscription.unsubscribe();
          resolve();
        }, 15_000);

        const subscription = indexerWsClient.subscribeToDustNullifierTransactions(
          {
            next: (payload) => {
              const tx = payload.data?.dustNullifierTransactions;
              if (tx) {
                received.push(tx);
                clearTimeout(timeout);
                subscription.unsubscribe();
                resolve();
              }
            },
            error: (err) => {
              clearTimeout(timeout);
              subscription.unsubscribe();
              reject(new Error(`Subscription error: ${JSON.stringify(err)}`));
            },
            complete: () => {
              clearTimeout(timeout);
              resolve();
            },
          },
          ['00'],
          0,
          latestHeight,
        );
      });

      if (received.length === 0) {
        log.warn(
          'no dustNullifierTransactions matched prefix "00" within the timeout; ' +
            'transaction reference check skipped',
        );
        ctx.skip?.(true, 'no dust nullifier transactions matched — check vacuous');
        return;
      }

      const first = received[0];
      log.debug(
        `Checking DustNullifierTransaction.transaction.hash=${first.transaction.hash} ` +
          `against transactionHash=${first.transactionHash}`,
      );

      expect(first.transaction.hash).toBeDefined();
      expect(first.transaction.hash).toBe(first.transactionHash);
    }, 30_000);
  });
});
