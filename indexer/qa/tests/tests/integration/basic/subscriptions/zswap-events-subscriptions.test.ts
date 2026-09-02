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

import '@utils/logging/test-logging-hooks';
import { IndexerWsClient } from '@utils/indexer/websocket-client';
import { EventCoordinator } from '@utils/event-coordinator';
import { ZswapLedgerEventSchema } from '@utils/indexer/graphql/schema';
import {
  collectValidZswapEvents,
  collectZswapEventError,
} from '../../../shared/zswap-events-utils';

describe('zswap ledger event subscriptions', () => {
  let indexerWsClient: IndexerWsClient;
  let eventCoordinator: EventCoordinator;

  beforeEach(async () => {
    indexerWsClient = new IndexerWsClient();
    eventCoordinator = new EventCoordinator();
    await indexerWsClient.connectionInit();
  }, 30_000);

  afterEach(async () => {
    await indexerWsClient.connectionClose();
    eventCoordinator.clear();
  });

  describe('a subscription to zswap ledger events without offset (default replay)', () => {
    /**
     * Subscribing to ZswapLedger events without providing an offset should replay
     * historical events in the correct ledger order.
     *
     *  Note:
     * - Event IDs are allocated from a single global ledger sequence shared across DustLedger and ZswapLedger
     * - As a result, Dust ledger event IDs are not guaranteed to be contiguous.
     *
     * @given no zswap event offset parameters are provided
     * @when we subscribe to zswap ledger events
     * @then events must be applied sequentially in order
     * @and event IDs must be globally increasing and must not go backwards
     */
    test('streams events in ledger order', async () => {
      const received = await collectValidZswapEvents(indexerWsClient, eventCoordinator, 3);
      expect(received.length === 3, `Expected 3 events, got: ${received.length}`).toBe(true);
      const ids = received.map((e) => e.data!.zswapLedgerEvents.id);
      const inLedgerOrder = ids.every((id, i) => i === 0 || id > ids[i - 1]);
      expect(
        inLedgerOrder,
        `Zswap event IDs must be in ledger order (IDs must not go backwards), got: ${ids.join(', ')}`,
      ).toBe(true);
    });
  });

  describe('subscription with explicit offset', () => {
    /**
     * Subscribing to ZswapLedger events with an explicit offset should replay
     * historical events beginning from the provided event ID.
     *
     * @given a zswap ledger event offset parameter is provided
     * @when we subscribe to zswap ledger events with that offset
     * @then events must be applied sequentially in order
     * @and event IDs must be globally increasing and must not go backwards
     */
    test('streams events starting from the specified ID', async () => {
      const firstEvent = await collectValidZswapEvents(indexerWsClient, eventCoordinator, 3);
      const firstZswapId = firstEvent[0].data!.zswapLedgerEvents.id;
      const startId = Math.max(firstZswapId - 1, 0);
      const received = await collectValidZswapEvents(indexerWsClient, eventCoordinator, 3, startId);
      expect(received.length === 3, `Expected 3 events, got: ${received.length}`).toBe(true);

      const ids = received.map((e) => e.data!.zswapLedgerEvents.id);
      const inLedgerOrder = ids.every((id, i) => i === 0 || id > ids[i - 1]);
      expect(
        inLedgerOrder,
        `Zswap event IDs must be in ledger order (IDs must not go backwards), got: ${ids.join(', ')}`,
      ).toBe(true);
    });

    /**
     * Validates that all replayed zswap ledger events conform to the expected schema.
     *
     * @given a zswap ledger subscription with an explicit offset ID
     * @when historical zswap events are streamed starting from that offset
     * @then each received event must match the ZswapLedgerEventsUnionSchema definition
     */
    test('validates historical zswap events against schema', async () => {
      const received = await collectValidZswapEvents(indexerWsClient, eventCoordinator, 3);
      received
        .filter((msg) => msg.data?.zswapLedgerEvents)
        .forEach((msg) => {
          expect.soft(msg).toBeSuccess();

          const event = msg.data!.zswapLedgerEvents;
          const parsed = ZswapLedgerEventSchema.safeParse(event);
          expect(
            parsed.success,
            `Zswap ledger event schema validation failed:\n${JSON.stringify(parsed.error?.format(), null, 2)}`,
          ).toBe(true);
        });
    });
  });

  describe('subscription error handling', () => {
    /**
     * Subscribing with a query that references a nonexistent field should return
     * a GraphQL validation error instead of streaming zswap ledger events.
     *
     * @given a zswap ledger subscription whose selection set contains an unknown field
     * @when the subscription request is sent to the indexer GraphQL endpoint
     * @then the server must return a validation error indicating the field does not exist
     * @and no zswap ledger events should be streamed
     */
    test('should return an error for unknown field', async () => {
      const errorMessage = await collectZswapEventError(indexerWsClient, null, true);
      expect(errorMessage).toBe(`Unknown field "unknownField" on type "ZswapLedgerEvent".`);
    });

    /**
     * Providing a negative offset should result in an error response instead of
     *
     * @given a zswap ledger subscription with an explicit offset parameter
     * @when the offset value is negative
     * @then an error should be returned
     */
    test('rejects negative offset ID with an error', async () => {
      const errorMessage = await collectZswapEventError(indexerWsClient, { id: -50 });
      expect(errorMessage).toBe(`Failed to parse "Int": Invalid number`);
    });
  });
});
