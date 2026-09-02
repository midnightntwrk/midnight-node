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
import { IndexerWsClient } from '@utils/indexer/websocket-client';
import { extractSubscriptionErrorMessage } from '@utils/indexer/subscription-error';
import { ToolkitWrapper } from '@utils/toolkit/toolkit-wrapper';
import dataProvider from '@utils/testdata-provider';

// Toolkit cold-start can be slow on the first run when the image is pulled.
const TOOLKIT_STARTUP_TIMEOUT = 60_000;

/**
 * Coverage for midnight-indexer#984 / PR #1039
 * (`feat: avoid unnecessary scans of shielded transactions`).
 *
 * The `connect` mutation accepts an optional `ConnectOptions { startIndex: Int }`
 * input. When provided, the wallet-indexer is told to start scanning shielded
 * transactions from the given index instead of from genesis. The intended use
 * cases (per the issue) are:
 *   - brand-new wallet: pass the current tip → no historical scan happens;
 *   - wallet restoring with a known commitment-tree position: pass that index.
 *
 * Observable contract on `shieldedTransactions(sessionId)`:
 *   - the wallet's `ShieldedTransactionsProgress.highestCheckedZswapEndIndex`
 *     advances at least to `startIndex` immediately.
 */
// Skipped on `undeployed`: the local chain has too few zswap events to assert
// startIndex-based offset behaviour meaningfully. Re-enable once #1152 makes
// local chain data rich enough (or the test derives startIndex from the tip).
describe.skipIf(env.isUndeployedEnv())('wallet connect options (startIndex)', () => {
  let toolkit: ToolkitWrapper;
  let indexerWsClient: IndexerWsClient;

  beforeAll(async () => {
    toolkit = new ToolkitWrapper({});
    await toolkit.start();
  }, TOOLKIT_STARTUP_TIMEOUT);

  afterAll(async () => {
    await toolkit.stop();
  });

  beforeEach(async () => {
    indexerWsClient = new IndexerWsClient();
    await indexerWsClient.connectionInit();
  }, 30_000);

  afterEach(async () => {
    await indexerWsClient.connectionClose();
  });

  /**
   * Helper: open a transient session without options, wait for the first
   * `ShieldedTransactionsProgress`, and return its `highestZswapEndIndex` —
   * which is the exclusive upper bound on the chain's scanned transaction
   * cursor at this moment, i.e. the "current tip" for sync purposes. The
   * caller can pass this value as `startIndex` to a second session to make
   * the wallet-indexer skip every transaction <= the tip.
   *
   * The probe relies on `connectionClose()` (in finally) to clean up. We
   * deliberately do NOT call `closeWalletSession` here: when a session has
   * just been told the chain tip, it is server-side-caught-up and the
   * subsequent `disconnect` mutation race-condition'd a non-standard
   * response shape on QANET. Closing the WS achieves the same teardown.
   */
  async function readCurrentZswapTipIndex(viewingKey: string): Promise<number> {
    const probeWs = new IndexerWsClient();
    await probeWs.connectionInit();
    try {
      const probeSessionId = await probeWs.openWalletSession(viewingKey);
      return await new Promise<number>((resolve, reject) => {
        const timeout = setTimeout(
          () => reject(new Error('Timed out waiting for tip Progress event')),
          15_000,
        );
        const unsubscribe = probeWs.subscribeToShieldedTransactionEvents(
          {
            next: (payload) => {
              const event = payload.data?.shieldedTransactions;
              if (event?.__typename === 'ShieldedTransactionsProgress') {
                clearTimeout(timeout);
                unsubscribe();
                resolve(event.highestZswapEndIndex);
              }
            },
          },
          probeSessionId,
        );
      });
    } finally {
      await probeWs.connectionClose();
    }
  }

  describe('opening a session with startIndex', () => {
    /**
     * @given a valid viewing key and `options.startIndex = 0`
     * @when we open a wallet session
     * @then the indexer returns a valid session ID — `startIndex: 0` is the
     *       no-op equivalent of not passing options at all and must not be
     *       rejected as malformed.
     */
    test('should accept startIndex = 0 (no-op equivalent of unset options)', async () => {
      const seed = dataProvider.getFundingSeed();
      const viewingKey = await toolkit.showViewingKey(seed);

      const sessionId = await indexerWsClient.openWalletSession(viewingKey, {
        startIndex: 0,
      });

      expect(sessionId).toMatch(/^[a-f0-9]+$/);
      // No closeWalletSession: afterEach.connectionClose() releases the
      // session server-side. See readCurrentZswapTipIndex docstring for why.
    });

    /**
     * @given a valid viewing key and `options.startIndex` set to the current
     *        chain tip (the brand-new-wallet case from the issue)
     * @when we open a wallet session and subscribe to `shieldedTransactions`
     * @then the first `ShieldedTransactionsProgress` event reports
     *       `highestCheckedZswapEndIndex >= startIndex`, proving that the
     *       wallet-indexer skipped scanning the historical portion of the
     *       chain instead of catching up incrementally from 0.
     */
    test('should skip historical scan when startIndex equals the current tip', async (ctx) => {
      const seed = dataProvider.getFundingSeed();
      const viewingKey = await toolkit.showViewingKey(seed);

      const tipIndex = await readCurrentZswapTipIndex(viewingKey);
      log.debug(`tip zswap end index = ${tipIndex}`);
      // The probe is only meaningful if the chain has produced shielded
      // transactions. On an empty network the assertion below is vacuous;
      // skip with a clear message rather than passing trivially.
      if (tipIndex === 0) {
        log.warn('chain has no shielded transactions; skipping startIndex optimisation check');
        ctx.skip();
      }

      const sessionId = await indexerWsClient.openWalletSession(viewingKey, {
        startIndex: tipIndex,
      });
      log.debug(`opened session with startIndex=${tipIndex}, sessionId=${sessionId}`);

      const firstProgress = await new Promise<{
        __typename: 'ShieldedTransactionsProgress';
        highestZswapEndIndex: number;
        highestCheckedZswapEndIndex: number;
        highestRelevantZswapEndIndex: number;
      }>((resolve, reject) => {
        const timeout = setTimeout(
          () => reject(new Error('Timed out waiting for first Progress event')),
          15_000,
        );
        const unsubscribe = indexerWsClient.subscribeToShieldedTransactionEvents(
          {
            next: (payload) => {
              const event = payload.data?.shieldedTransactions;
              if (event?.__typename === 'ShieldedTransactionsProgress') {
                clearTimeout(timeout);
                unsubscribe();
                resolve(event);
              }
            },
            error: (err) => {
              clearTimeout(timeout);
              unsubscribe();
              reject(
                new Error(`subscription returned errors: ${extractSubscriptionErrorMessage(err)}`),
              );
            },
          },
          sessionId,
        );
      });

      log.debug(`first progress = ${JSON.stringify(firstProgress)}`);

      // Core invariant: the indexer reports it has *checked* at least up to
      // the provided startIndex without having actually scanned earlier txs.
      expect(
        firstProgress.highestCheckedZswapEndIndex,
        `highestCheckedZswapEndIndex (${firstProgress.highestCheckedZswapEndIndex}) ` +
          `should be >= startIndex (${tipIndex}); the optimisation appears not to apply`,
      ).toBeGreaterThanOrEqual(tipIndex);
    }, 30_000);

    /**
     * @given a valid viewing key and `options.startIndex` past the current tip
     * @when we open a wallet session and subscribe to `shieldedTransactions`
     * @then the indexer accepts the connection — startIndex beyond tip is a
     *       legitimate "fast-forward" use case (wallet pre-emptively trusts
     *       its local commitment tree). The subscription should emit a
     *       Progress event without crashing the worker.
     */
    test('should accept startIndex past the current tip (fast-forward)', async () => {
      const seed = dataProvider.getFundingSeed();
      const viewingKey = await toolkit.showViewingKey(seed);

      const tipIndex = await readCurrentZswapTipIndex(viewingKey);
      // 1M past tip is well beyond any realistic chain depth: exercises the
      // fast-forward semantics, not "near tip" semantics.
      const fastForward = tipIndex + 1_000_000;

      const sessionId = await indexerWsClient.openWalletSession(viewingKey, {
        startIndex: fastForward,
      });
      expect(sessionId).toMatch(/^[a-f0-9]+$/);

      const firstProgress = await new Promise<{
        __typename: 'ShieldedTransactionsProgress';
        highestZswapEndIndex: number;
        highestCheckedZswapEndIndex: number;
        highestRelevantZswapEndIndex: number;
      }>((resolve, reject) => {
        const timeout = setTimeout(
          () => reject(new Error('Timed out waiting for first Progress event')),
          15_000,
        );
        const unsubscribe = indexerWsClient.subscribeToShieldedTransactionEvents(
          {
            next: (payload) => {
              const event = payload.data?.shieldedTransactions;
              if (event?.__typename === 'ShieldedTransactionsProgress') {
                clearTimeout(timeout);
                unsubscribe();
                resolve(event);
              }
            },
            error: (err) => {
              clearTimeout(timeout);
              unsubscribe();
              reject(
                new Error(`subscription returned errors: ${extractSubscriptionErrorMessage(err)}`),
              );
            },
          },
          sessionId,
        );
      });

      // The worker emitted Progress without crashing or surfacing errors.
      expect(firstProgress.__typename).toBe('ShieldedTransactionsProgress');
    }, 30_000);
  });
});
