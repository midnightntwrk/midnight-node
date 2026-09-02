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

import { randomBytes } from 'crypto';
import type { TestContext } from 'vitest';
import '@utils/logging/test-logging-hooks';
import { IndexerWsClient } from '@utils/indexer/websocket-client';
import { extractSubscriptionErrorMessage } from '@utils/indexer/subscription-error';
import { BLOCKS_SUBSCRIPTION_FROM_LATEST_BLOCK } from '@utils/indexer/graphql/subscriptions';
import { ToolkitWrapper } from '@utils/toolkit/toolkit-wrapper';

// Shipped defaults from the indexer-api `infra.api.quota` configuration.
const PER_CONNECTION_CAP = 20;
const PER_SESSION_RATE_LIMIT = 10;

// The rate-limit token bucket refills continuously at the per-minute limit spread
// across 60 seconds — one token every 6 seconds at the default of 10 per minute.
const TOKEN_REFILL_WAIT_MS = 8_000;
// A live shielded subscription re-polls progress within its 30s base interval ±20% jitter.
const PROGRESS_AFTER_REJECTION_TIMEOUT_MS = 60_000;
const REGISTRATION_WAIT_MS = 1_000;
const RESPONSE_TIMEOUT_MS = 5_000;
const TOOLKIT_STARTUP_TIMEOUT = 60_000;
const WS_OPEN = 1;

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

describe('subscription quotas', () => {
  let client: IndexerWsClient;

  beforeEach(async () => {
    client = new IndexerWsClient();
    await client.connectionInit();
  }, 30_000);

  afterEach(async () => {
    await client.connectionClose();
  });

  describe('per-connection concurrent subscription cap', () => {
    /**
     * The indexer enforces a per-connection concurrent active subscription cap
     * via the SubscriptionGuard acquired at each resolver entry. Default cap is
     * 20 (configurable via `infra.api.quota.max_concurrent_per_connection`).
     * The 21st concurrent subscription on a single WebSocket connection must
     * be rejected with a client error, while the WebSocket connection itself
     * stays open and the existing 20 subscriptions continue unaffected.
     *
     * @given a single WebSocket connection with 20 active block subscriptions
     * @when a 21st block subscription is opened on the same connection
     * @then the 21st subscription returns a client error mentioning the limit
     * @and the WebSocket connection stays open
     */
    test('should reject a subscription beyond the per-connection cap', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Subscription', 'Quota', 'Negative'] };

      const cleanups: Array<() => void> = [];

      for (let i = 0; i < PER_CONNECTION_CAP; i++) {
        const idx = i + 1;
        const cleanup = client.subscribe(BLOCKS_SUBSCRIPTION_FROM_LATEST_BLOCK, {
          next: () => {
            /* drain quietly */
          },
          error: (err) => {
            throw new Error(
              `Subscription #${idx} of ${PER_CONNECTION_CAP} unexpectedly errored: ${extractSubscriptionErrorMessage(err)}`,
            );
          },
        });
        cleanups.push(cleanup);
      }

      await sleep(REGISTRATION_WAIT_MS);

      const rejected = await new Promise<string | null>((resolve) => {
        const timeout = setTimeout(() => resolve(null), RESPONSE_TIMEOUT_MS);
        const cleanupExtra = client.subscribe(BLOCKS_SUBSCRIPTION_FROM_LATEST_BLOCK, {
          next: () => {
            clearTimeout(timeout);
            cleanupExtra();
            resolve(null);
          },
          error: (err) => {
            clearTimeout(timeout);
            resolve(extractSubscriptionErrorMessage(err));
          },
        });
      });

      expect(
        rejected,
        `Expected the ${PER_CONNECTION_CAP + 1}th subscription to be rejected, but no error was returned within ${RESPONSE_TIMEOUT_MS}ms`,
      ).not.toBeNull();
      expect(rejected!.toLowerCase()).toContain('subscription limit exceeded');
      expect(
        client.getState(),
        `WebSocket connection should stay open after a single subscription is rejected, got ${IndexerWsClient.getStateName(client.getState())}`,
      ).toBe(WS_OPEN);

      cleanups.forEach((fn) => fn());
    }, 30_000);

    /**
     * The per-connection counter is decremented when a subscription ends,
     * via the `SubscriptionGuard` Drop. After 20 active subscriptions are
     * established and one is closed, the freed slot must allow a new
     * subscription to start on the same connection.
     *
     * @given 20 active block subscriptions on a single connection
     * @when one of the active subscriptions is closed
     * @then a new subscription opened on the same connection succeeds
     */
    test('should free a slot when an active subscription is closed', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Subscription', 'Quota'] };

      const cleanups: Array<() => void> = [];

      for (let i = 0; i < PER_CONNECTION_CAP; i++) {
        const cleanup = client.subscribe(BLOCKS_SUBSCRIPTION_FROM_LATEST_BLOCK, {
          next: () => {
            /* drain quietly */
          },
        });
        cleanups.push(cleanup);
      }

      await sleep(REGISTRATION_WAIT_MS);

      const closed = cleanups.shift();
      closed!();

      await sleep(REGISTRATION_WAIT_MS);

      const succeeded = await new Promise<boolean>((resolve) => {
        const timeout = setTimeout(() => resolve(false), RESPONSE_TIMEOUT_MS);
        const cleanupExtra = client.subscribe(BLOCKS_SUBSCRIPTION_FROM_LATEST_BLOCK, {
          next: () => {
            clearTimeout(timeout);
            cleanupExtra();
            resolve(true);
          },
          error: () => {
            clearTimeout(timeout);
            resolve(false);
          },
        });
      });

      expect(
        succeeded,
        `Expected a new subscription to succeed after one of the ${PER_CONNECTION_CAP} active subscriptions was closed (slot should be freed via Drop)`,
      ).toBe(true);

      cleanups.forEach((fn) => fn());
    }, 30_000);
  });

  describe('per-session subscription creation rate limit', () => {
    let toolkit: ToolkitWrapper;

    beforeAll(async () => {
      toolkit = new ToolkitWrapper({});
      await toolkit.start();
    }, TOOLKIT_STARTUP_TIMEOUT);

    afterAll(async () => {
      await toolkit.stop();
    });

    // The rate limit is enforced before wallet resolution, so a session for a
    // random (empty) wallet is sufficient — no on-chain data is needed.
    const openRandomWalletSession = async (): Promise<string> => {
      const viewingKey = await toolkit.showViewingKey(randomBytes(32).toString('hex'));
      return client.openWalletSession(viewingKey);
    };

    // Starts `count` shielded subscriptions for the session in rapid succession,
    // collecting unexpected rejection messages instead of throwing so callers can
    // assert on them.
    const openShieldedSubscriptions = (sessionId: string, count: number) => {
      const errors: string[] = [];
      const cleanups: Array<() => void> = [];
      for (let i = 0; i < count; i++) {
        const cleanup = client.subscribeToShieldedTransactionEvents(
          {
            next: () => {
              /* drain quietly */
            },
            error: (err) => {
              errors.push(extractSubscriptionErrorMessage(err));
            },
          },
          sessionId,
        );
        cleanups.push(cleanup);
      }
      return { errors, cleanups };
    };

    // Attempts one more shielded subscription creation for the session. Resolves with
    // 'accepted' once its first progress event arrives, the normalized rejection
    // message if the creation is refused, or 'no response' if neither happens in time.
    const attemptShieldedSubscription = (sessionId: string): Promise<string> => {
      return new Promise((resolve) => {
        const timeout = setTimeout(() => resolve('no response'), RESPONSE_TIMEOUT_MS);
        const cleanup = client.subscribeToShieldedTransactionEvents(
          {
            next: () => {
              clearTimeout(timeout);
              cleanup();
              resolve('accepted');
            },
            error: (err) => {
              clearTimeout(timeout);
              resolve(extractSubscriptionErrorMessage(err));
            },
          },
          sessionId,
        );
      });
    };

    /**
     * The indexer enforces a per-session subscription creation rate limit via a
     * token bucket, default 10 creations per minute (configurable via
     * `infra.api.quota.max_session_subscriptions_per_minute`). The 11th shielded
     * subscription creation for the same session within the same minute must be
     * rejected with a client error, while the WebSocket connection stays open.
     *
     * @given a wallet session opened with a valid viewing key
     * @when 10 shielded subscriptions for that session are created in rapid succession
     * @and an 11th creation is attempted within the same minute
     * @then the 11th creation is rejected with a client error mentioning the limit
     * @and the WebSocket connection stays open
     */
    test('should reject shielded subscription creations beyond the per-session rate limit', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Subscription', 'Quota', 'Wallet', 'Negative'] };

      const sessionId = await openRandomWalletSession();

      // The burst plus the registration wait stays well under the 6s single-token
      // refill period, so no token refills before the extra attempt.
      const { errors, cleanups } = openShieldedSubscriptions(sessionId, PER_SESSION_RATE_LIMIT);
      await sleep(REGISTRATION_WAIT_MS);
      expect(
        errors,
        `The first ${PER_SESSION_RATE_LIMIT} subscription creations should all be accepted, got: ${errors.join('; ')}`,
      ).toHaveLength(0);

      const outcome = await attemptShieldedSubscription(sessionId);

      // 'per-session rate limit' pins the rejection to the rate-limit guardrail;
      // the per-connection cap shares the same 'subscription limit exceeded' wrapper.
      expect(
        outcome.toLowerCase(),
        `Expected creation ${PER_SESSION_RATE_LIMIT + 1} within the same minute to be rejected by the per-session rate limit, got: ${outcome}`,
      ).toContain('per-session rate limit');
      expect(
        client.getState(),
        `WebSocket connection should stay open after a rate-limited creation, got ${IndexerWsClient.getStateName(client.getState())}`,
      ).toBe(WS_OPEN);

      cleanups.forEach((fn) => fn());
    }, 60_000);

    /**
     * The rate-limit token bucket refills continuously at the per-minute limit
     * spread across 60 seconds — one token every 6 seconds at the default of 10
     * per minute. After the session's allowance is exhausted, waiting longer than
     * one refill period must allow a new subscription creation to succeed, so a
     * throttled wallet is never permanently locked out.
     *
     * @given a wallet session whose subscription creation allowance is exhausted
     * @when a new shielded subscription is created after an 8s wait, longer than the 6s single-token refill period
     * @then the new subscription is accepted
     */
    test('should allow a new shielded subscription after the rate limit refills', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Subscription', 'Quota', 'Wallet'] };

      const sessionId = await openRandomWalletSession();

      const { errors, cleanups } = openShieldedSubscriptions(sessionId, PER_SESSION_RATE_LIMIT);
      await sleep(REGISTRATION_WAIT_MS);
      expect(
        errors,
        `The first ${PER_SESSION_RATE_LIMIT} subscription creations should all be accepted, got: ${errors.join('; ')}`,
      ).toHaveLength(0);

      const rejected = await attemptShieldedSubscription(sessionId);
      expect(
        rejected.toLowerCase(),
        `The creation allowance should be exhausted before the refill wait, got: ${rejected}`,
      ).toContain('per-session rate limit');

      await sleep(TOKEN_REFILL_WAIT_MS);

      const outcome = await attemptShieldedSubscription(sessionId);
      expect(
        outcome,
        `Expected a creation ${TOKEN_REFILL_WAIT_MS}ms after exhaustion to be accepted, got: ${outcome}`,
      ).toBe('accepted');

      cleanups.forEach((fn) => fn());
    }, 60_000);

    /**
     * A rate-limit rejection must not disturb subscriptions that are already
     * active: a live shielded subscription keeps streaming its periodic progress
     * updates after further creations for the same session were rejected.
     *
     * @given one live shielded subscription for a wallet session
     * @and the session's creation allowance exhausted by further creations
     * @when an additional creation is rejected with the rate-limit error
     * @then the live subscription still receives progress updates after the rejection
     * @and the WebSocket connection stays open
     */
    test('should keep a live shielded subscription streaming while creations are rate-limited', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Subscription', 'Quota', 'Wallet'] };

      const sessionId = await openRandomWalletSession();

      const progressTimestamps: number[] = [];
      const liveErrors: string[] = [];
      const stopLive = client.subscribeToShieldedTransactionEvents(
        {
          next: (payload) => {
            if (payload.data?.shieldedTransactions?.__typename === 'ShieldedTransactionsProgress') {
              progressTimestamps.push(Date.now());
            }
          },
          error: (err) => {
            liveErrors.push(extractSubscriptionErrorMessage(err));
          },
        },
        sessionId,
      );

      const { errors, cleanups } = openShieldedSubscriptions(sessionId, PER_SESSION_RATE_LIMIT - 1);
      await sleep(REGISTRATION_WAIT_MS);
      expect(
        errors,
        `The live subscription plus ${PER_SESSION_RATE_LIMIT - 1} further creations should all be accepted, got: ${errors.join('; ')}`,
      ).toHaveLength(0);

      const outcome = await attemptShieldedSubscription(sessionId);
      expect(
        outcome.toLowerCase(),
        `Expected the creation beyond the allowance to be rejected by the per-session rate limit, got: ${outcome}`,
      ).toContain('per-session rate limit');
      const rejectionTime = Date.now();

      // The live subscription re-polls progress within its 30s base interval ±20%
      // jitter; wait for a progress update delivered strictly after the rejection.
      const deadline = rejectionTime + PROGRESS_AFTER_REJECTION_TIMEOUT_MS;
      while (Date.now() < deadline && !progressTimestamps.some((t) => t > rejectionTime)) {
        await sleep(500);
      }

      expect(
        progressTimestamps.some((t) => t > rejectionTime),
        `Expected the live subscription to receive a progress update within ${PROGRESS_AFTER_REJECTION_TIMEOUT_MS}ms after the rejection`,
      ).toBe(true);
      expect(
        liveErrors,
        `The live subscription should not error while creations are rate-limited, got: ${liveErrors.join('; ')}`,
      ).toHaveLength(0);
      expect(
        client.getState(),
        `WebSocket connection should stay open, got ${IndexerWsClient.getStateName(client.getState())}`,
      ).toBe(WS_OPEN);

      stopLive();
      cleanups.forEach((fn) => fn());
    }, 120_000);
  });
});
