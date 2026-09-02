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

// Integration tests for c2m-bridge GraphQL subscriptions: bridgeEvents and
// bridgeBalance (#942).
//
// Subscription mechanics: bridgeEvents backfills-then-live-tails — it replays
// matching historical events from the optional `from` cursor (an event id) and
// then streams new ones; there is no progress sentinel and no toBlock bound.
// bridgeBalance(address) emits the current balance immediately on connect, then
// re-emits on every relevant event. A claim is a BridgeClaimTransaction surfaced
// via the unshieldedTransactions query — there is no bridgeClaims subscription.
//
// Test-data reality (2026-07): bridge events are cross-chain and only
// BridgeUserTransfer has data, only on devnet. The immediate zero-balance frame
// needs only the surface; the historical replay needs a real UserTransfer and
// ctx.skips otherwise. Variant/recipient filters, reconnection continuity and
// balance-update-on-event stay test.todo until a data source can produce them.
//   test.todo → needs bridge data not yet producible in any test environment.
//   test.skip → blocked on a specific in-flight feature (noted inline).
//
// Tracking: https://github.com/midnightntwrk/midnight-indexer/issues/942

import log from '@utils/logging/logger';
import { env } from 'environment/model';
import type { TestContext } from 'vitest';
import '@utils/logging/test-logging-hooks';
import {
  IndexerWsClient,
  BridgeEventSubscriptionResponse,
  BridgeBalanceSubscriptionResponse,
} from '@utils/indexer/websocket-client';
import type { BridgeUserTransfer } from '@utils/indexer/indexer-types';

const EMPTY_RECIPIENT = '0'.repeat(64);
const ZERO_U128 = '0'.repeat(32);

// The bridgeEvents subscription selects id/recipient only on the UserTransfer
// fragment. Cross-variant filter/resume cases pass this override so id is present
// on every variant and recipient on both recipient-bearing variants. `from` is a
// separate operation because the client picks the query by presence of `from`.
const SUB_ALL_FIELDS_FROM = `
subscription BridgeEventsFromAll($FROM: Int, $RECIPIENT: HexEncoded, $VARIANT: BridgeEventVariant) {
  bridgeEvents(from: $FROM, recipient: $RECIPIENT, variant: $VARIANT) {
    __typename
    ... on BridgeUserTransfer { id blockHeight recipient }
    ... on BridgeReserveTransfer { id blockHeight }
    ... on BridgeInvalidTransfer { id blockHeight }
    ... on BridgeUnapprovedTransfer { id blockHeight recipient }
    ... on BridgeSubminimalFlushTransfer { id blockHeight }
  }
}`;

let surfacePresent = false;
let sampleUserTransfer: BridgeUserTransfer | null = null;

function safeUnsubscribe(unsubscribe: () => void): void {
  try {
    unsubscribe();
  } catch (error) {
    log.debug(`Ignoring unsubscribe error during teardown: ${String(error)}`);
  }
}

/**
 * Opens a bridgeEvents subscription, collects every frame delivered during the
 * backfill-then-live-tail window, and resolves once the stream goes idle (no new
 * frame for `idleMs`). Mirrors the settle/idle/hard-timeout pattern of the
 * replay test so the filter/resume cases share one race-free collector.
 *
 * Resolves with whatever was collected when the stream goes idle or the hard
 * timeout fires; rejects only on a subscription error.
 */
function collectBridgeEventFrames(
  wsClient: IndexerWsClient,
  opts: { from?: number; recipient?: string; variant?: string },
  queryOverride?: string,
  timing: { idleMs: number; hardMs: number } = { idleMs: 5_000, hardMs: 30_000 },
): Promise<BridgeEventSubscriptionResponse[]> {
  const received: BridgeEventSubscriptionResponse[] = [];
  return new Promise<BridgeEventSubscriptionResponse[]>((resolve, reject) => {
    let settled = false;
    let idleTimer: ReturnType<typeof setTimeout> | undefined;
    let unsubscribe = () => {};
    const settle = (handler: () => void) => {
      if (settled) return;
      settled = true;
      clearTimeout(idleTimer);
      handler();
    };
    const hardTimeout = setTimeout(() => {
      safeUnsubscribe(unsubscribe);
      settle(() => resolve(received));
    }, timing.hardMs);
    const resetIdle = () => {
      clearTimeout(idleTimer);
      idleTimer = setTimeout(() => {
        clearTimeout(hardTimeout);
        safeUnsubscribe(unsubscribe);
        settle(() => resolve(received));
      }, timing.idleMs);
    };

    const subscription = wsClient.subscribeToBridgeEvents(
      {
        next: (payload) => {
          received.push(payload);
          resetIdle();
        },
        error: (error) => {
          clearTimeout(hardTimeout);
          safeUnsubscribe(unsubscribe);
          settle(() => reject(new Error(`Subscription error: ${JSON.stringify(error)}`)));
        },
      },
      opts,
      queryOverride,
    );
    unsubscribe = subscription.unsubscribe;
  });
}

// Extracts the bridge event payloads (single event per frame) from collected frames.
const framesToEvents = (frames: BridgeEventSubscriptionResponse[]) =>
  frames.map((f) => f.data!.bridgeEvents);
// Sorted, de-duplicated ids from a set of collected events (variants without a
// selected id are dropped).
const sortedUniqueIds = (events: { id?: number }[]): number[] =>
  [...new Set(events.map((e) => e.id).filter((id): id is number => id !== undefined))].sort(
    (a, b) => a - b,
  );

/**
 * Probes the bridge query surface over HTTP to decide surface presence and pick
 * a sample UserTransfer. Kept self-contained (a raw fetch, not the query client)
 * so this subscription PR does not depend on the #941 query plumbing.
 */
async function probeBridgeSurface(): Promise<{
  present: boolean;
  sample: BridgeUserTransfer | null;
}> {
  const apiVersion = process.env.INDEXER_API_VERSION?.trim() || 'v4';
  const url = `${env.getIndexerHttpBaseURL()}/api/${apiVersion}/graphql`;
  const query = `query {
    bridgeEvents(variant: USER_TRANSFER, limit: 1) {
      __typename
      ... on BridgeUserTransfer { id blockHeight midnightTxHash cardanoTxHash amount recipient }
    }
  }`;
  try {
    const response = await fetch(url, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ query }),
      signal: AbortSignal.timeout(15_000),
    });
    const body = (await response.json()) as {
      data?: { bridgeEvents: BridgeUserTransfer[] };
      errors?: unknown[];
    };
    if (!response.ok || body.errors || !body.data) {
      return { present: false, sample: null };
    }
    const sample =
      body.data.bridgeEvents.find((e) => e.__typename === 'BridgeUserTransfer') ?? null;
    return { present: true, sample };
  } catch (error) {
    log.warn(`Bridge surface probe failed: ${String(error)}`);
    return { present: false, sample: null };
  }
}

describe.skipIf(env.isUndeployedEnv())('bridge subscriptions', () => {
  let wsClient: IndexerWsClient;

  beforeAll(async () => {
    const probe = await probeBridgeSurface();
    if (!probe.present) {
      log.warn(`Bridge surface not present on ${env.getCurrentEnvironmentName()}; skipping`);
      return;
    }
    surfacePresent = true;
    sampleUserTransfer = probe.sample;
  }, 30_000);

  beforeEach(async () => {
    wsClient = new IndexerWsClient();
    await wsClient.connectionInit();
  }, 30_000);

  afterEach(async () => {
    await wsClient.connectionClose();
  });

  describe('bridgeEvents', () => {
    /**
     * @given an environment with at least one indexed BridgeUserTransfer
     * @when a bridgeEvents subscription is opened with from = <sampleId - 1>
     * @then the known UserTransfer is replayed with id >= sampleId in ascending order
     */
    test('should replay historical events from the given cursor id', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Subscription', 'Bridge', 'UserTransfer'] };
      if (!surfacePresent) return ctx.skip();
      if (!sampleUserTransfer) return ctx.skip(true, 'no BridgeUserTransfer data on this env');

      const fromCursor = Math.max(0, sampleUserTransfer.id - 1);
      const received: BridgeEventSubscriptionResponse[] = [];

      await new Promise<void>((resolve, reject) => {
        let settled = false;
        let idleTimer: ReturnType<typeof setTimeout> | undefined;
        let unsubscribe = () => {};
        const settle = (handler: () => void) => {
          if (settled) return;
          settled = true;
          clearTimeout(idleTimer);
          handler();
        };
        const hardTimeout = setTimeout(() => {
          safeUnsubscribe(unsubscribe);
          if (received.length > 0) {
            settle(resolve);
          } else {
            settle(() => reject(new Error('no bridge events replayed within timeout')));
          }
        }, 30_000);
        const resetIdle = () => {
          clearTimeout(idleTimer);
          idleTimer = setTimeout(() => {
            clearTimeout(hardTimeout);
            safeUnsubscribe(unsubscribe);
            settle(resolve);
          }, 5_000);
        };

        const subscription = wsClient.subscribeToBridgeEvents(
          {
            next: (payload) => {
              received.push(payload);
              resetIdle();
            },
            error: (error) => {
              clearTimeout(hardTimeout);
              safeUnsubscribe(unsubscribe);
              settle(() => reject(new Error(`Subscription error: ${JSON.stringify(error)}`)));
            },
          },
          { from: fromCursor },
        );
        unsubscribe = subscription.unsubscribe;
      });

      const events = received.map((r) => r.data!.bridgeEvents);
      expect(events.length).toBeGreaterThan(0);
      const ids = events.map((e) => e.id!).filter((id) => id !== undefined);
      expect(ids).toContain(sampleUserTransfer.id);
      expect([...ids]).toEqual([...ids].sort((a, b) => a - b));
    }, 60_000);

    /**
     * @given a chain with bridge events for a known recipient
     * @when a bridgeEvents subscription is opened with recipient = <knownAddress>
     * @then every delivered event echoes the matching recipient
     */
    test('should only deliver events matching the recipient filter', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Subscription', 'Bridge', 'Filter'] };
      if (!surfacePresent) return ctx.skip();
      if (!sampleUserTransfer) return ctx.skip(true, 'no BridgeUserTransfer data on this env');

      const recipient = sampleUserTransfer.recipient;
      const frames = await collectBridgeEventFrames(
        wsClient,
        { from: 0, recipient },
        SUB_ALL_FIELDS_FROM,
      );

      const events = framesToEvents(frames);
      expect(events.length).toBeGreaterThan(0);
      for (const event of events) {
        expect('recipient' in event ? event.recipient : undefined).toBe(recipient);
      }
    }, 60_000);

    /**
     * @given a chain with multiple event variants
     * @when a bridgeEvents subscription is opened with variant = USER_TRANSFER
     * @then only BridgeUserTransfer events are delivered
     */
    test('should only deliver events matching the variant filter', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Subscription', 'Bridge', 'Filter', 'UserTransfer'] };
      if (!surfacePresent) return ctx.skip();
      if (!sampleUserTransfer) return ctx.skip(true, 'no BridgeUserTransfer data on this env');

      const frames = await collectBridgeEventFrames(wsClient, {
        from: 0,
        variant: 'USER_TRANSFER',
      });

      const events = framesToEvents(frames);
      expect(events.length).toBeGreaterThan(0);
      for (const event of events) {
        expect(event.__typename).toBe('BridgeUserTransfer');
      }
    }, 60_000);

    /**
     * @given the ordered ids of all events replayed from a from:0 subscription
     * @when a second subscription resumes from a mid-stream cursor id
     * @then the resumed ids are a contiguous tail of the full order — ascending,
     *       de-duplicated (no duplication) and gap-free
     */
    test('should resume from cursor without gap or duplication on reconnection', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Subscription', 'Bridge', 'Resume'] };
      if (!surfacePresent) return ctx.skip();

      const allFrames = await collectBridgeEventFrames(wsClient, { from: 0 }, SUB_ALL_FIELDS_FROM);
      const allIds = sortedUniqueIds(framesToEvents(allFrames));
      if (allIds.length < 3) {
        return ctx.skip(true, `need >= 3 replayable events to resume, found ${allIds.length}`);
      }

      // Resume from a mid-stream cursor so the tail is a strict subset.
      const cursor = allIds[1];
      const resumedFrames = await collectBridgeEventFrames(
        wsClient,
        { from: cursor },
        SUB_ALL_FIELDS_FROM,
      );
      const resumedIds = sortedUniqueIds(framesToEvents(resumedFrames));

      expect(resumedIds.length).toBeGreaterThan(0);
      // No duplication: sortedUniqueIds already dedups, so a duplicate would have
      // shrunk it below the raw frame count for the selected variants.
      const rawResumedIds = framesToEvents(resumedFrames)
        .map((e) => e.id)
        .filter((id): id is number => id !== undefined);
      expect(rawResumedIds).toHaveLength(resumedIds.length);
      // Gap-free contiguous tail of the full ordered id list.
      expect(resumedIds).toEqual(allIds.slice(allIds.length - resumedIds.length));
      // Actually resumed partway, so it is shorter than the full replay.
      expect(resumedIds.length).toBeLessThan(allIds.length);
    }, 90_000);

    /**
     * @given a Cardano-backed chain that has produced an UnapprovedTransfer
     * @when a bridgeEvents subscription replays from the start
     * @then at least one BridgeUnapprovedTransfer frame is delivered
     *
     * Was blocked (#940) on the node's approval governance; that logic ships in
     * node >= 2.0.0-rc.3, so UnapprovedTransfer is now produced. Skips gracefully
     * where the chain has no such event.
     */
    test('should deliver BridgeUnapprovedTransfer events via subscription', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Subscription', 'Bridge', 'Unapproved'] };
      if (!surfacePresent) return ctx.skip();

      const frames = await collectBridgeEventFrames(wsClient, { from: 0 }, SUB_ALL_FIELDS_FROM);
      const events = framesToEvents(frames);
      const hasUnapproved = events.some((e) => e.__typename === 'BridgeUnapprovedTransfer');
      if (!hasUnapproved) {
        return ctx.skip(true, 'no UnapprovedTransfer event on this chain');
      }
      expect(hasUnapproved).toBe(true);
    }, 60_000);
  });

  describe('claims via unshieldedTransactions', () => {
    // There is no bridgeClaims subscription. A bridge claim is a
    // BridgeClaimTransaction surfaced via the unshieldedTransactions query, so
    // claim coverage belongs with the unshielded-transaction tests.
    /**
     * @given a chain containing a CardanoBridge claim for a known recipient
     * @when unshieldedTransactions is observed for that recipient
     * @then the claim appears as a BridgeClaimTransaction
     */
    test.todo(
      'should observe claims as BridgeClaimTransaction via the unshieldedTransactions query',
    );
  });

  describe('bridgeBalance', () => {
    /**
     * @given a 32-byte all-zeros address with no bridge activity
     * @when a bridgeBalance subscription is opened for that address
     * @then the first emitted frame has deposited, claimed and balance all zero
     */
    test('should emit zero balance immediately for an address with no bridge activity', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Subscription', 'Bridge', 'Balance', 'Negative'] };
      if (!surfacePresent) return ctx.skip();

      const firstFrame = await new Promise<BridgeBalanceSubscriptionResponse>((resolve, reject) => {
        let unsubscribe = () => {};
        const timeout = setTimeout(() => {
          safeUnsubscribe(unsubscribe);
          reject(new Error('no bridgeBalance frame received within timeout'));
        }, 15_000);

        const subscription = wsClient.subscribeToBridgeBalance(
          {
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
          },
          EMPTY_RECIPIENT,
        );
        unsubscribe = subscription.unsubscribe;
      });

      expect(firstFrame).toBeSuccess();
      expect(firstFrame.data?.bridgeBalance).toEqual({
        deposited: ZERO_U128,
        claimed: ZERO_U128,
        balance: ZERO_U128,
      });
    }, 30_000);

    /**
     * @given a subscription to bridgeBalance(address: <knownAddress>)
     * @when a UserTransfer for that address is indexed
     * @then a new BridgeBalance with deposited > 0 is pushed
     */
    test.todo('should push an updated BridgeBalance when a relevant UserTransfer is indexed');

    /**
     * @given a subscribed address with a prior UserTransfer balance
     * @when a CardanoBridge claim for that address is indexed
     * @then balance reflects the net remaining-claimable, reaching zero once fully claimed
     */
    test.todo('should push an updated BridgeBalance when a claim reduces the balance');
  });
});
