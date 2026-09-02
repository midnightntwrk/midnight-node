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

// Integration tests for the c2m-bridge pool observability subscription:
// bridgePoolUpdates (#944).
//
// bridgePoolUpdates takes no arguments. It emits the current BridgePoolSummary
// immediately on connect (with newEvent null), then live-tails a BridgePoolUpdate
// frame on every newly indexed pool-affecting event (Reserve or Treasury). There
// is no cursor / historical-replay parameter.
//
// Test-data reality (2026-07): no reserve or treasury events exist on any
// environment, so the immediate-snapshot frame is testable (its pool totals are
// legitimately zero) but the live-push cases need pool-affecting events that no
// environment can currently produce.
//   test.todo → needs pool-affecting event generation not available in any env.
//   test.skip → blocked on an in-flight feature: UNAPPROVED on approval
//               governance (#940); SUBMINIMAL_FLUSH on a triggerable flush
//               threshold (node team).
//
// Tracking: https://github.com/midnightntwrk/midnight-indexer/issues/944

import log from '@utils/logging/logger';
import { env } from 'environment/model';
import type { TestContext } from 'vitest';
import '@utils/logging/test-logging-hooks';
import {
  IndexerWsClient,
  BridgePoolUpdateSubscriptionResponse,
} from '@utils/indexer/websocket-client';
import { IndexerHttpClient } from '@utils/indexer/http-client';
import { BRIDGE_TREASURY_REASONS } from '@utils/indexer/indexer-types';

const httpClient = new IndexerHttpClient();
const ZERO_U128 = '0'.repeat(32);

let surfacePresent = false;

function safeUnsubscribe(unsubscribe: () => void): void {
  try {
    unsubscribe();
  } catch (error) {
    log.debug(`Ignoring unsubscribe error during teardown: ${String(error)}`);
  }
}

describe.skipIf(env.isUndeployedEnv())('bridge pool subscription', () => {
  let wsClient: IndexerWsClient;

  beforeAll(async () => {
    const probe = await httpClient.getBridgePoolSummary();
    if (probe.errors || !probe.data) {
      log.warn(`Bridge pool surface not present on ${env.getCurrentEnvironmentName()}; skipping`);
      return;
    }
    surfacePresent = true;
  }, 30_000);

  beforeEach(async () => {
    wsClient = new IndexerWsClient();
    await wsClient.connectionInit();
  }, 30_000);

  afterEach(async () => {
    await wsClient.connectionClose();
  });

  describe('bridgePoolUpdates', () => {
    /**
     * @given no pool-affecting bridge events have been indexed
     * @when a bridgePoolUpdates subscription is opened
     * @then the first frame has newEvent null and a pool summary with zero
     *       reserveTotal, zero subminimumTxCount and the three treasury reasons
     */
    test('should emit current pool summary immediately on subscribe', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Subscription', 'Bridge', 'Pool'] };
      if (!surfacePresent) return ctx.skip();

      const firstFrame = await new Promise<BridgePoolUpdateSubscriptionResponse>(
        (resolve, reject) => {
          let unsubscribe = () => {};
          const timeout = setTimeout(() => {
            safeUnsubscribe(unsubscribe);
            reject(new Error('no bridgePoolUpdates frame received within timeout'));
          }, 15_000);

          const subscription = wsClient.subscribeToBridgePoolUpdates({
            next: (payload) => {
              clearTimeout(timeout);
              safeUnsubscribe(unsubscribe);
              resolve(payload);
            },
            error: (error) => {
              clearTimeout(timeout);
              safeUnsubscribe(unsubscribe);
              reject(new Error(`Subscription error: ${JSON.stringify(error)}`));
            },
          });
          unsubscribe = subscription.unsubscribe;
        },
      );

      expect(firstFrame).toBeSuccess();
      const update = firstFrame.data!.bridgePoolUpdates;
      expect(update.newEvent).toBeNull();
      expect(update.pool.reserveTotal).toBe(ZERO_U128);
      expect(update.pool.subminimumTxCount).toBe(0);
      const reasons = update.pool.treasuryByReason.map((t) => t.reason).sort();
      expect(reasons).toEqual([...BRIDGE_TREASURY_REASONS].sort());
    }, 30_000);

    /**
     * @given a live bridgePoolUpdates subscription
     * @when a new ReserveTransfer event is indexed
     * @then a frame arrives with newEvent BridgeReserveTransfer and an updated pool.reserveTotal
     */
    test.todo('should push an update when a new ReserveTransfer is indexed');

    /**
     * @given a live bridgePoolUpdates subscription
     * @when a new InvalidTransfer event is indexed
     * @then a frame arrives with newEvent BridgeInvalidTransfer and an increased INVALID aggregate
     */
    test.todo('should push an update when a new InvalidTransfer is indexed');

    /**
     * @given a live bridgePoolUpdates subscription
     * @when a UserTransfer event is indexed
     * @then no frame is pushed (a user transfer affects a user address, not a pool)
     */
    test.todo('should not push a pool update for UserTransfer events (user address, not pool)');

    /**
     * @given 3 ReserveTransfer events with known amounts indexed while subscribed
     * @when a frame arrives per pool-affecting event
     * @then pool.reserveTotal carries the running cumulative sum across frames
     */
    test.todo('should carry a running pool total in each frame (cumulative consistency)');

    // Blocked: approval governance not landed yet (#940), so UnapprovedTransfer
    // cannot be produced.
    test.skip('should push an update when an UnapprovedTransfer is indexed', () => {});

    // Blocked: SubminimalFlush threshold not yet confirmed (see header note, #944).
    test.skip('should push an update and increment subminimumTxCount when a flush is indexed', () => {});
  });
});
