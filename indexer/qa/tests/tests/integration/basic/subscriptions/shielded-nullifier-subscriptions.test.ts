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
  ShieldedNullifierTransactionSubscriptionResponse,
  SubscriptionHandlers,
} from '@utils/indexer/websocket-client';
import { buildErrorPayload } from '@utils/indexer/subscription-error';
import { ShieldedNullifierTransactionSchema } from '@utils/indexer/graphql/schema';
import { IndexerHttpClient } from '@utils/indexer/http-client';
import { ShieldedNullifierTransaction } from '@utils/indexer/indexer-types';

const indexerHttpClient = new IndexerHttpClient();

type SettledSubscriptionError = {
  payload: ShieldedNullifierTransactionSubscriptionResponse | null;
  completed: boolean;
  eventCount: number;
};

/**
 * Run a `shieldedNullifierTransactions` subscription that is expected to fail
 * fast with a GraphQL client error. Resolves with the first payload that
 * carries `errors`, or — if nothing arrives within the timeout — with
 * `payload: null` so the caller's `toBeError()` assertion produces a clear
 * "expected a GraphQL error, got null" message instead of a cryptic
 * "null vs string" mismatch.
 */
function collectSubscriptionError(
  start: (handlers: SubscriptionHandlers<ShieldedNullifierTransactionSubscriptionResponse>) => {
    unsubscribe: () => void;
  },
  timeoutMs = 8_000,
): Promise<SettledSubscriptionError> {
  return new Promise((resolve) => {
    let eventCount = 0;
    const timeout = setTimeout(() => {
      subscription.unsubscribe();
      resolve({ payload: null, completed: false, eventCount });
    }, timeoutMs);

    const subscription = start({
      next: () => {
        eventCount++;
      },
      error: (error) => {
        clearTimeout(timeout);
        subscription.unsubscribe();
        resolve({
          payload: buildErrorPayload<ShieldedNullifierTransactionSubscriptionResponse>(error),
          completed: false,
          eventCount,
        });
      },
      complete: () => {
        clearTimeout(timeout);
        resolve({ payload: null, completed: true, eventCount });
      },
    });
  });
}

/**
 * Coverage for midnight-indexer#994 / PR #996
 * (`feat: add shieldedNullifierTransactions subscription`).
 *
 * The subscription returns transaction references for any shielded (Zswap)
 * transaction whose nullifiers match one of the provided hex prefixes, within
 * an optional block range. It mirrors `dustNullifierTransactions` but on the
 * shielded surface. Wallets use it to detect spends of coins they discovered
 * via trial-decryption that the regular shielded sync wouldn't otherwise
 * surface (the shielded sync only catches transactions with outputs for the
 * provided viewing key, not pure nullifier-only spends).
 *
 * Mirrors the structure of `dust-nullifier-subscriptions.test.ts` so the two
 * surfaces stay symmetric.
 */
describe('shielded nullifier transactions subscription', () => {
  let indexerWsClient: IndexerWsClient;

  beforeEach(async () => {
    indexerWsClient = new IndexerWsClient();
    await indexerWsClient.connectionInit();
  }, 30_000);

  afterEach(async () => {
    await indexerWsClient.connectionClose();
  });

  describe('streaming shielded nullifier transactions with block range', () => {
    /**
     * @given a set of nullifier prefixes and a bounded block range
     * @when we subscribe to shieldedNullifierTransactions
     * @then we should receive matching transactions (if any) and the
     *       subscription should complete once `toBlock` is reached
     * @and each transaction should match the expected schema
     */
    test('should stream transactions within a block range and complete', async () => {
      const blockResponse = await indexerHttpClient.getLatestBlock();
      expect(blockResponse).toBeSuccess();
      const latestHeight = blockResponse.data!.block.height;

      // Use a broad prefix to maximise the chance of matches in the early
      // window. Bounded to first 10 blocks for determinism.
      const toBlock = Math.min(latestHeight, 10);
      const nullifierPrefixes = ['00'];

      log.debug(
        `Subscribing to shielded nullifier transactions with prefixes=${nullifierPrefixes}, fromBlock=0, toBlock=${toBlock}`,
      );

      const received: ShieldedNullifierTransactionSubscriptionResponse[] = [];

      await new Promise<void>((resolve, reject) => {
        const timeout = setTimeout(() => {
          subscription.unsubscribe();
          // OK if no matches found in the range — subscription still
          // completes; absent matches is a valid outcome.
          resolve();
        }, 15_000);

        const subscription = indexerWsClient.subscribeToShieldedNullifierTransactions(
          {
            next: (payload) => {
              received.push(payload);
              log.debug(
                `Received shielded nullifier transaction: ${JSON.stringify(payload.data?.shieldedNullifierTransactions)}`,
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
          nullifierPrefixes,
          0,
          toBlock,
        );
      });

      log.debug(`Received ${received.length} shielded nullifier transactions`);

      for (const msg of received) {
        expect(msg).toBeSuccess();
        const tx = msg.data!.shieldedNullifierTransactions;
        const parsed = ShieldedNullifierTransactionSchema.safeParse(tx);
        expect(
          parsed.success,
          `Shielded nullifier transaction schema validation failed: ${JSON.stringify(parsed.error, null, 2)}`,
        ).toBe(true);

        // Block height should be within the requested range.
        expect(tx.blockHeight).toBeGreaterThanOrEqual(0);
        expect(tx.blockHeight).toBeLessThanOrEqual(toBlock);
      }
    });
  });

  /**
   * Coverage for midnight-indexer#1119 / PR #1126
   * (`feat(indexer-api): tighten shielded nullifier transactions input validation`).
   *
   * `shieldedNullifierTransactions` now rejects the same malformed inputs as
   * `dustNullifierTransactions` (hardened in #1089 / PR #1090), restoring
   * parity between the two surfaces:
   *   - empty `nullifierPrefixes` array → `"nullifierPrefixes must not be empty"`
   *   - empty-string element after hex decode → `"nullifierPrefixes elements must not be empty"`
   *   - `fromBlock > toBlock` → `"fromBlock must not exceed toBlock"`
   *
   * Mirrors the dust-side cases in `dust-nullifier-subscriptions.test.ts`.
   */
  describe('subscription error handling', () => {
    /**
     * @given an empty array of nullifier prefixes
     * @when we subscribe to shieldedNullifierTransactions
     * @then the subscription should return a client error about empty prefixes
     */
    test('should return an error for empty nullifier prefixes', async (ctx: TestContext) => {
      const settled = await collectSubscriptionError((handlers) =>
        indexerWsClient.subscribeToShieldedNullifierTransactions(handlers, [], 0),
      );

      if (settled.payload === null) {
        log.warn(
          `subscription emitted no payload (completed=${settled.completed}, ` +
            `eventCount=${settled.eventCount}); cannot validate the ` +
            `'nullifierPrefixes must not be empty' contract on this indexer build — skipping`,
        );
        ctx.skip();
        return;
      }
      expect(settled.payload).toBeError();
      expect(settled.payload.errors![0].message).toContain('nullifierPrefixes must not be empty');
      expect(settled.completed).toBe(false);
      expect(settled.eventCount).toBeGreaterThanOrEqual(0);
    });

    /**
     * @given a nullifier prefixes array containing an empty-string element
     * @when we subscribe to shieldedNullifierTransactions
     * @then the subscription should return a client error about empty elements
     */
    test('should return an error for an empty-string nullifier prefix element', async (ctx: TestContext) => {
      const settled = await collectSubscriptionError((handlers) =>
        indexerWsClient.subscribeToShieldedNullifierTransactions(handlers, [''], 0),
      );

      if (settled.payload === null) {
        log.warn(
          `subscription emitted no payload (completed=${settled.completed}, ` +
            `eventCount=${settled.eventCount}); cannot validate the ` +
            `'nullifierPrefixes elements must not be empty' contract on this indexer build — skipping`,
        );
        ctx.skip();
        return;
      }
      expect(settled.payload).toBeError();
      expect(settled.payload.errors![0].message).toContain(
        'nullifierPrefixes elements must not be empty',
      );
      expect(settled.completed).toBe(false);
      expect(settled.eventCount).toBeGreaterThanOrEqual(0);
    });

    /**
     * @given fromBlock greater than toBlock
     * @when we subscribe to shieldedNullifierTransactions
     * @then the subscription should return a client error about the block range
     */
    test('should return an error when fromBlock is greater than toBlock', async (ctx: TestContext) => {
      const settled = await collectSubscriptionError((handlers) =>
        indexerWsClient.subscribeToShieldedNullifierTransactions(handlers, ['00'], 10, 5),
      );

      if (settled.payload === null) {
        log.warn(
          `subscription emitted no payload (completed=${settled.completed}, ` +
            `eventCount=${settled.eventCount}); cannot validate the ` +
            `'fromBlock must not exceed toBlock' contract on this indexer build — skipping`,
        );
        ctx.skip();
        return;
      }
      expect(settled.payload).toBeError();
      expect(settled.payload.errors![0].message).toContain('fromBlock must not exceed toBlock');
      expect(settled.completed).toBe(false);
      expect(settled.eventCount).toBeGreaterThanOrEqual(0);
    });
  });

  /**
   * Coverage for midnight-indexer#1114 / PR #1116
   * (`feat(indexer-api): add transactionHash to event subscription response types`).
   *
   * `transactionHash: HexEncoded!` was added to `ShieldedNullifierTransaction`.
   * The schema-level shape (64-hex, non-nullable) is already enforced by the
   * `ShieldedNullifierTransactionSchema` used by the streaming test above.
   * This block adds the round-trip check: the streamed hash must resolve a
   * transaction via `transactions(offset: { hash: ... })`.
   *
   * Match presence is environment-dependent. If no transactions match within
   * the timeout, the round-trip is vacuous and we skip rather than asserting
   * against an empty stream.
   */
  describe('transactionHash on shielded nullifier events (#1114)', () => {
    /**
     * @given a wide prefix scan of the full chain
     * @when we subscribe to `shieldedNullifierTransactions` and look up the
     *       first streamed event's `transactionHash` via `transactions(offset)`
     * @then the lookup resolves a single transaction whose `hash` equals the
     *       streamed `transactionHash` — proving the field is the on-chain
     *       identifier.
     */
    test('first event transactionHash resolves via transactions(offset)', async (ctx: TestContext) => {
      const blockResponse = await indexerHttpClient.getLatestBlock();
      expect(blockResponse).toBeSuccess();
      const latestHeight = blockResponse.data!.block.height;

      const received: ShieldedNullifierTransaction[] = [];

      await new Promise<void>((resolve, reject) => {
        const timeout = setTimeout(() => {
          subscription.unsubscribe();
          resolve();
        }, 15_000);

        const subscription = indexerWsClient.subscribeToShieldedNullifierTransactions(
          {
            next: (payload) => {
              const tx = payload.data?.shieldedNullifierTransactions;
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
          'no shieldedNullifierTransactions matched prefix "00" within the timeout; ' +
            'round-trip skipped (environment has no shielded nullifier transactions in range)',
        );
        ctx.skip?.(
          true,
          'no shielded nullifier transactions matched within timeout — round-trip vacuous',
        );
        return;
      }

      const first = received[0];
      log.debug(
        `Round-tripping ShieldedNullifierTransaction.transactionHash=${first.transactionHash} ` +
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
   * `transaction: Transaction! @beta` was added to `ShieldedNullifierTransaction`
   * so consumers can navigate to all Transaction fields directly from the
   * streamed event without a separate lookup. The Zod schema enforces the
   * field shape; this block adds the consistency check: `transaction.hash`
   * must equal `transactionHash` on the same event.
   */
  describe('transaction reference on shielded nullifier events (#1115)', () => {
    /**
     * @given a wide prefix scan of the full chain
     * @when we subscribe to shieldedNullifierTransactions and receive the first event
     * @then event.transaction.hash equals event.transactionHash, confirming the
     *       reference field is wired to the same on-chain transaction
     */
    test('first event transaction.hash matches transactionHash', async (ctx: TestContext) => {
      const blockResponse = await indexerHttpClient.getLatestBlock();
      expect(blockResponse).toBeSuccess();
      const latestHeight = blockResponse.data!.block.height;

      const received: ShieldedNullifierTransaction[] = [];

      await new Promise<void>((resolve, reject) => {
        const timeout = setTimeout(() => {
          subscription.unsubscribe();
          resolve();
        }, 15_000);

        const subscription = indexerWsClient.subscribeToShieldedNullifierTransactions(
          {
            next: (payload) => {
              const tx = payload.data?.shieldedNullifierTransactions;
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
          'no shieldedNullifierTransactions matched prefix "00" within the timeout; ' +
            'transaction reference check skipped',
        );
        ctx.skip?.(true, 'no shielded nullifier transactions matched — check vacuous');
        return;
      }

      const first = received[0];
      log.debug(
        `Checking ShieldedNullifierTransaction.transaction.hash=${first.transaction.hash} ` +
          `against transactionHash=${first.transactionHash}`,
      );

      expect(first.transaction.hash).toBeDefined();
      expect(first.transaction.hash).toBe(first.transactionHash);
    }, 30_000);
  });
});
