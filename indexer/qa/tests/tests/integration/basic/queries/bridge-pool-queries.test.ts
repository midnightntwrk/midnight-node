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

// Integration tests for c2m-bridge pool observability queries: bridgePoolSummary,
// bridgeReserveInflows, bridgeTreasuryInflows (#944).
//
// Pool observability tracks where NIGHT flows when bridge transactions are
// processed: the Reserve pool (ReserveTransfer) and the Treasury (Invalid,
// Unapproved, SubminimalFlush). Per-address balance (UserTransfer/claims) is
// covered by #941.
//
// Test-data reality (2026-07): no reserve or treasury events exist on any
// environment (the only bridge data anywhere is a single UserTransfer on
// devnet, which does not touch these pools). So the pool totals are legitimately
// all-zero everywhere, and the inflow lists are empty. That makes the well-formed
// zero-state and empty-list cases real, executable tests; the cases that need
// non-zero aggregation stay test.todo until a data source can produce reserve /
// treasury events.
//   test.todo → needs reserve/treasury event data not producible in any env yet.
//   test.skip → blocked on an in-flight feature: UNAPPROVED on approval
//               governance (#940); SUBMINIMAL_FLUSH on a triggerable flush
//               threshold (node team).
//
// Tracking: https://github.com/midnightntwrk/midnight-indexer/issues/944

import log from '@utils/logging/logger';
import { env } from 'environment/model';
import type { TestContext } from 'vitest';
import '@utils/logging/test-logging-hooks';
import { IndexerHttpClient } from '@utils/indexer/http-client';
import { BRIDGE_TREASURY_REASONS } from '@utils/indexer/indexer-types';

const httpClient = new IndexerHttpClient();

const ZERO_U128 = '0'.repeat(32);
// An early block that predates any possible bridge event, for the atBlock snapshot.
const EARLY_BLOCK = 1;

let surfacePresent = false;

describe.skipIf(env.isUndeployedEnv())('bridge pool queries', () => {
  beforeAll(async () => {
    const probe = await httpClient.getBridgePoolSummary();
    if (probe.errors || !probe.data) {
      log.warn(`Bridge pool surface not present on ${env.getCurrentEnvironmentName()}; skipping`);
      return;
    }
    surfacePresent = true;
  }, 30_000);

  describe('bridgePoolSummary', () => {
    /**
     * @given an environment with no reserve or treasury bridge events indexed
     * @when bridgePoolSummary is queried
     * @then reserveTotal and every treasuryByReason total are the zero-value hex
     *       string, subminimumTxCount is 0, and the three treasury reasons
     *       (INVALID, UNAPPROVED, SUBMINIMAL_FLUSH) are each present
     */
    test('should return zero pool totals when no reserve or treasury events are indexed', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Query', 'Bridge', 'Pool'] };
      if (!surfacePresent) return ctx.skip();

      const response = await httpClient.getBridgePoolSummary();

      expect(response).toBeSuccess();
      const pool = response.data!.bridgePoolSummary;
      expect(pool.reserveTotal).toBe(ZERO_U128);
      expect(pool.subminimumTxCount).toBe(0);

      const reasons = pool.treasuryByReason.map((t) => t.reason).sort();
      expect(reasons).toEqual([...BRIDGE_TREASURY_REASONS].sort());
      for (const aggregate of pool.treasuryByReason) {
        expect(aggregate.total).toBe(ZERO_U128);
      }
    });

    /**
     * @given an environment where at least one bridge event has been indexed
     * @when bridgePoolSummary is queried
     * @then lastEventBlockHeight is a positive integer
     *
     * On environments with no bridge events at all, lastEventBlockHeight is null
     * and the case is skipped.
     */
    test('should expose lastEventBlockHeight as a positive integer where bridge events exist', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Query', 'Bridge', 'Pool'] };
      if (!surfacePresent) return ctx.skip();

      const response = await httpClient.getBridgePoolSummary();
      expect(response).toBeSuccess();
      const lastEventBlockHeight = response.data!.bridgePoolSummary.lastEventBlockHeight;

      if (lastEventBlockHeight === null) {
        return ctx.skip(true, 'no bridge events on this env — lastEventBlockHeight is null');
      }
      expect(Number.isInteger(lastEventBlockHeight)).toBe(true);
      expect(lastEventBlockHeight).toBeGreaterThan(0);
    });

    /**
     * @given a block that predates any bridge event (block 1)
     * @when bridgePoolSummary(atBlock: 1) is queried
     * @then the snapshot has null lastEventBlockHeight and zero totals, confirming
     *       the point-in-time snapshot excludes later events
     */
    test('should return an empty snapshot when atBlock predates all bridge events', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Query', 'Bridge', 'Pool', 'ByHeight'] };
      if (!surfacePresent) return ctx.skip();

      const response = await httpClient.getBridgePoolSummary(EARLY_BLOCK);

      expect(response).toBeSuccess();
      const pool = response.data!.bridgePoolSummary;
      expect(pool.lastEventBlockHeight).toBeNull();
      expect(pool.reserveTotal).toBe(ZERO_U128);
      expect(pool.subminimumTxCount).toBe(0);
    });

    /**
     * @given a chain with N ReserveTransfer events of known amounts
     * @when bridgePoolSummary is queried
     * @then reserveTotal equals the sum of all ReserveTransfer.amount values
     */
    test.todo('should set reserveTotal to the cumulative sum of ReserveTransfer amounts');

    /**
     * @given a chain with InvalidTransfer and UnapprovedTransfer events
     * @when bridgePoolSummary is queried
     * @then treasuryByReason contains INVALID and UNAPPROVED entries with correct totals
     */
    test.todo('should aggregate treasury inflows separately by INVALID and UNAPPROVED reason');

    /**
     * @given reserve/treasury events exist at blocks B1 and B2 (B1 < B2)
     * @when bridgePoolSummary(atBlock: B1) and (atBlock: B2) are queried
     * @then only events up to and including the given block contribute, and B2 totals exceed B1
     */
    test.todo('should accumulate more pool inflow at a later atBlock than an earlier one');

    // Blocked: requires crossing the subminimum accumulation threshold to produce
    // a SubminimalFlushTransfer. Pending confirmation of whether the threshold is
    // a configurable genesis parameter (node team) so a flush can be triggered.
    // Tracking: https://github.com/midnightntwrk/midnight-indexer/issues/944
    test.skip('should count SubminimalFlushTransfer.count in subminimumTxCount', () => {});

    // Blocked: approval governance not landed yet, so UnapprovedTransfer cannot
    // be produced. Tracking: https://github.com/midnightntwrk/midnight-indexer/issues/940
    test.skip('should aggregate UnapprovedTransfer amounts under UNAPPROVED treasury reason', () => {});
  });

  describe('bridgeReserveInflows', () => {
    /**
     * @given an environment with no ReserveTransfer events indexed
     * @when bridgeReserveInflows is queried
     * @then the response is successful and returns an empty array
     */
    test('should return an empty list when no ReserveTransfer events are indexed', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Query', 'Bridge', 'Pool', 'Negative'] };
      if (!surfacePresent) return ctx.skip();

      const response = await httpClient.getBridgeReserveInflows();

      expect(response).toBeSuccess();
      expect(response.data?.bridgeReserveInflows).toEqual([]);
    });

    /**
     * @given a chain with ReserveTransfer events across multiple blocks
     * @when bridgeReserveInflows(blockHeightFrom: B1, blockHeightTo: B2) is queried
     * @then only events with blockHeight in [B1, B2] are returned
     */
    test.todo('should return ReserveTransfer events within the specified block range');

    /**
     * @given at least one ReserveTransfer is indexed
     * @when bridgeReserveInflows(limit: 1) is queried
     * @then the event exposes id, blockHeight, midnightTxHash, cardanoTxHash, amount
     */
    test.todo('should return events with correct BridgeReserveTransfer field shape');

    /**
     * @given at least 3 ReserveTransfer events are indexed
     * @when queried with limit=2 offset=0 and limit=2 offset=1
     * @then results are consistent and ids are in ascending order
     */
    test.todo('should paginate ReserveTransfer events with offset and limit');
  });

  describe('bridgeTreasuryInflows', () => {
    /**
     * @given an environment with no treasury-redirected events indexed
     * @when bridgeTreasuryInflows is queried
     * @then the response is successful and returns an empty array
     */
    test('should return an empty list when no treasury-redirected events are indexed', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Query', 'Bridge', 'Pool', 'Negative'] };
      if (!surfacePresent) return ctx.skip();

      const response = await httpClient.getBridgeTreasuryInflows();

      expect(response).toBeSuccess();
      expect(response.data?.bridgeTreasuryInflows).toEqual([]);
    });

    /**
     * @given a chain with InvalidTransfer events
     * @when bridgeTreasuryInflows is queried with no reason filter
     * @then the results include BridgeInvalidTransfer events
     */
    test.todo('should return all treasury event types when no reason filter is given');

    /**
     * @given a chain with both Invalid and Reserve events
     * @when bridgeTreasuryInflows(reason: INVALID) is queried
     * @then every returned event has __typename BridgeInvalidTransfer
     */
    test.todo('should return only BridgeInvalidTransfer when reason=INVALID');

    /**
     * @given treasury events exist at blocks B1 and B3
     * @when bridgeTreasuryInflows(blockHeightFrom: B1, blockHeightTo: B2) where B2 < B3
     * @then only events at B1 are returned (B3 is outside the range)
     */
    test.todo('should respect blockHeightFrom and blockHeightTo filters for treasury inflows');

    // Blocked: approval governance not landed yet (#940).
    test.skip('should return only BridgeUnapprovedTransfer when reason=UNAPPROVED', () => {});

    // Blocked: SubminimalFlush threshold not yet confirmed (see header note, #944).
    test.skip('should return only BridgeSubminimalFlushTransfer when reason=SUBMINIMAL_FLUSH', () => {});
  });
});
