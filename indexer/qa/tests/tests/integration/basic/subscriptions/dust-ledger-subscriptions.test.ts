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
import {
  collectValidDustLedgerEvents,
  collectDustLedgerEventError,
} from '../../../shared/dust-ledger-utils';
import { EventCoordinator } from '@utils/event-coordinator';
import { DustLedgerEventsUnionSchema } from '@utils/indexer/graphql/schema';

describe('dust ledger event subscriptions', () => {
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

  describe('a subscription to dust ledger events without offset (default replay)', () => {
    /**
     * Subscribing to DustLedger events without providing an offset should replay
     * historical events in the correct ledger order.
     *
     *  Note:
     * - Event IDs are allocated from a single global ledger sequence shared across DustLedger and ZswapLedger
     * - As a result, Dust ledger event IDs are not guaranteed to be contiguous.
     *
     * @given no dust event offset parameters are provided
     * @when we subscribe to dust ledger events
     * @then events must be applied sequentially in order
     * @and event IDs must be globally increasing and must not go backwards
     */
    test('streams events in ledger order', async () => {
      const received = await collectValidDustLedgerEvents(indexerWsClient, eventCoordinator, 3);

      expect(received.length === 3, `Expected 3 events, got: ${received.length}`).toBe(true);
      const ids = received.map((e) => e.data!.dustLedgerEvents.id);
      const inLedgerOrder = ids.every((id, i) => i === 0 || id > ids[i - 1]);
      expect(
        inLedgerOrder,
        `Dust event IDs must be delivered in ledger order (IDs must not go backwards), got: ${ids.join(', ')}`,
      ).toBe(true);
    });
  });

  describe('subscription with explicit offset', () => {
    /**
     * Subscribing to DustLedger events with an explicit offset should replay
     * historical events beginning from the provided event ID.
     *
     * @given a dust event offset parameter is provided
     * @when we subscribe to dust ledger events with that offset
     * @then events must be applied sequentially in order
     * @and event IDs must be globally increasing and must not go backwards
     */
    test('streams events in ledger order starting from the specified ID', async () => {
      const firstEvent = await collectValidDustLedgerEvents(indexerWsClient, eventCoordinator, 2);
      const latestId = firstEvent[0].data!.dustLedgerEvents.maxId;
      const startId = Math.max(latestId - 20, 0);
      const received = await collectValidDustLedgerEvents(
        indexerWsClient,
        eventCoordinator,
        2,
        startId,
      );
      expect(received.length === 2, `Expected 2 events, got: ${received.length}`).toBe(true);

      const ids = received.map((e) => e.data!.dustLedgerEvents.id);
      const inLedgerOrder = ids.every((id, i) => i === 0 || id > ids[i - 1]);
      expect(
        inLedgerOrder,
        `Dust ledger event IDs must be in ledger order (IDs must not go backwards), got: ${ids.join(', ')}`,
      ).toBe(true);
    });

    /**
     * Validates that all replayed dust ledger events conform to the expected schema.
     *
     * @given a dust ledger subscription with an explicit offset ID
     * @when historical dust events are streamed starting from that offset
     * @then each received event must match the DustLedgerEventsUnionSchema definition
     */
    test('validates historical dust events against schema', async () => {
      const firstEvent = await collectValidDustLedgerEvents(indexerWsClient, eventCoordinator, 3);
      const latestId = firstEvent[0].data!.dustLedgerEvents.maxId;

      const fromId = Math.max(latestId - 20, 0);
      const received = await collectValidDustLedgerEvents(
        indexerWsClient,
        eventCoordinator,
        3,
        fromId,
      );

      received
        .filter((msg) => msg.data?.dustLedgerEvents)
        .forEach((msg) => {
          expect.soft(msg).toBeSuccess();

          const event = msg.data!.dustLedgerEvents;
          const parsed = DustLedgerEventsUnionSchema.safeParse(event);
          expect(
            parsed.success,
            `Dust ledger event schema validation failed:\n${JSON.stringify(parsed.error?.format(), null, 2)}`,
          ).toBe(true);
        });
    });
  });

  describe('subscription error handling', () => {
    /**
     * Subscribing with a query that references a nonexistent field should return
     * a GraphQL validation error instead of streaming dust ledger events.
     *
     * @given a dust ledger subscription whose selection set contains an unknown field
     * @when the subscription request is sent to the indexer GraphQL endpoint
     * @then the server must return a validation error indicating the field does not exist
     * @and no dust ledger events should be streamed
     */
    test('should return an error for unknown field', async () => {
      const errorMessage = await collectDustLedgerEventError(indexerWsClient, null, true);
      expect(errorMessage).toBe(`Unknown field "unknownField" on type "DustLedgerEvent".`);
    });

    /**
     * Providing a negative offset should result in an error response instead of
     *
     * @given a dust ledger subscription with an explicit offset parameter
     * @when the offset value is negative
     * @then an error should be returned
     */
    test('rejects negative offset ID with an error', async () => {
      const errorMessage = await collectDustLedgerEventError(indexerWsClient, { id: -50 });
      expect(errorMessage).toBe(`Failed to parse "Int": Invalid number`);
    });
  });
});
