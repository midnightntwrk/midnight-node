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

import {
  SubscriptionHandlers,
  DustLedgerEventSubscriptionResponse,
  IndexerWsClient,
} from '@utils/indexer/websocket-client';
import { extractSubscriptionErrorMessage } from '@utils/indexer/subscription-error';
import { EventCoordinator } from '@utils/event-coordinator';
import log from '@utils/logging/logger';

/**
 * Helper to subscribe to dust ledger events and collect a specific number of valid responses.
 * Supports optional ID-based historical replay via `fromId`.
 */
export async function collectValidDustLedgerEvents(
  indexerWsClient: IndexerWsClient,
  eventCoordinator: EventCoordinator,
  expectedCount: number,
  fromId?: number,
  timeoutMs: number = 10_000,
): Promise<DustLedgerEventSubscriptionResponse[]> {
  const received: DustLedgerEventSubscriptionResponse[] = [];
  const eventName = `${expectedCount} DustLedger Events`;

  const handler = {
    next: (payload: DustLedgerEventSubscriptionResponse) => {
      if (received.length >= expectedCount) return;

      received.push(payload);
      log.debug(
        `Received event ${received.length}/${expectedCount}:\n${JSON.stringify(payload, null, 2)}`,
      );
      if (received.length == expectedCount) {
        eventCoordinator.notify(eventName);
        log.debug(`${expectedCount} Dust Ledger events received`);
      }
    },
  };

  const offset = fromId ? { id: fromId } : undefined;
  const subscription = indexerWsClient.subscribeToDustLedgerEvents(handler, offset);

  await eventCoordinator.waitForAll([eventName], timeoutMs);
  subscription.unsubscribe();
  return received;
}

/**
 * Helper to subscribe to dust ledger events and capture GraphQL error responses.
 * Used for testing invalid variables (e.g. negative offsets) or invalid fields.
 */
export async function collectDustLedgerEventError(
  indexerWsClient: IndexerWsClient,
  variables: Record<string, unknown> | null,
  unknownField: boolean = false,
): Promise<string> {
  return new Promise((resolve) => {
    const validQuery = `
      subscription DustLedgerEvents($id: Int) {
        dustLedgerEvents(id: $id) {
          id
        }
      }
    `;

    const invalidFieldQuery = `
      subscription DustLedgerEvents {
        dustLedgerEvents {
          unknownField
        }
      }
    `;

    const query = unknownField ? invalidFieldQuery : validQuery;

    let resolved = false;

    const handler: SubscriptionHandlers<unknown> = {
      next: (payload) => {
        if (resolved) return;
        if (typeof payload === 'object' && payload !== null && 'errors' in payload) {
          const p = payload as { errors: { message: string }[] };
          resolved = true;
          subscription.unsubscribe();
          clearTimeout(timeout);
          resolve(p.errors[0].message);
        }
      },
      error: (err) => {
        if (resolved) return;
        resolved = true;
        subscription.unsubscribe();
        clearTimeout(timeout);
        resolve(extractSubscriptionErrorMessage(err));
      },
    };

    let offset: { id: number } | undefined;
    if (variables?.id) {
      offset = { id: variables.id as number };
    }

    const subscription = indexerWsClient.subscribeToDustLedgerEvents(
      handler as SubscriptionHandlers<DustLedgerEventSubscriptionResponse>,
      offset,
      query,
    );

    const timeout = setTimeout(() => {
      if (resolved) return;
      resolved = true;
      subscription.unsubscribe();
      resolve('Timeout: No error received');
    }, 3000);
  });
}
