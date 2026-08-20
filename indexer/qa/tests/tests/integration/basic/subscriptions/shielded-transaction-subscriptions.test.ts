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
import log from '@utils/logging/logger';
import { env } from 'environment/model';
import '@utils/logging/test-logging-hooks';
import { TestContext } from 'vitest';
import {
  IndexerWsClient,
  SubscriptionHandlers,
  GraphQLCloseSessionMessage,
  ShieldedTxSubscriptionResponse,
} from '@utils/indexer/websocket-client';
import { buildErrorPayload } from '@utils/indexer/subscription-error';
import { generateSyntheticViewingKey } from '@utils/bech32-codec';
import { ToolkitWrapper } from '@utils/toolkit/toolkit-wrapper';
import { IndexerHttpClient } from '@utils/indexer/http-client';
import {
  MerkleTreeCollapsedUpdateSchema,
  ShieldedTransactionEventSchema,
} from '@utils/indexer/graphql/schema';
import dataProvider from '@utils/testdata-provider';

// This is longer because it might take some time when
// a new verion of the toolkit image is available and
// it needs to be pulled the first time
const TOOLKIT_STARTUP_TIMEOUT = 60_000;

describe('shielded transaction subscriptions', () => {
  let randomSeed: string;
  let toolkit: ToolkitWrapper;
  let indexerWsClient: IndexerWsClient;

  beforeAll(async () => {
    // Initialise the toolkit wrapper
    toolkit = new ToolkitWrapper({});
    await toolkit.start();
  }, TOOLKIT_STARTUP_TIMEOUT);

  afterAll(async () => {
    await toolkit.stop();
  });

  beforeEach(async () => {
    // Initialise a random seed used for the viewing key operations
    randomSeed = randomBytes(32).toString('hex');

    // Initialise the indexer websocket client and connect to it
    indexerWsClient = new IndexerWsClient();
    await indexerWsClient.connectionInit();
  }, 30_000);

  afterEach(async () => {
    // Close the indexer websocket client
    await indexerWsClient.connectionClose();
  });

  describe('opening a session with viewing key', async () => {
    /**
     * Opening a session with a valid viewing key returns a session ID
     *
     * Note: The only requirement is the viewing key is valid and matches the
     * target network is meant for. In essence, it might be a viewing key for
     * a wallet that doesn't exist, but that is ok because that is enough to open
     * a session. Then if the wallet doesn't exist (i.e. no relevant transactions),
     * the subscription will not stream any transaction data, that's all!
     *
     * @given a valid viewing key
     * @when we open a session with that viewing key
     * @then Indexer should return a session ID
     */
    test('should return a session ID, given a valid viewing key', async () => {
      log.info(`randomSeed = ${randomSeed}`);
      const viewingKey = await toolkit.showViewingKey(randomSeed);
      log.debug(`viewingKey = ${viewingKey}`);

      return indexerWsClient
        .openWalletSession(viewingKey)
        .then((sessionId) => {
          log.debug(`Received session id = ${sessionId}`);
          expect(sessionId).toMatch(/^[a-f0-9]+$/);
        })
        .catch((error) => {
          log.error(error);
          throw new Error(error);
        });
    });

    /**
     * Opening a session with unsupported hex format viewing key returns an error
     *
     * @given an unsupported hex format viewing key
     * @when we open a session with that viewing key
     * @then Indexer should return an error
     */
    test('should return an error, given an unsupported hex format viewing key', async () => {
      // Hex viewing key are no longer supported and should be rejected by indexer
      const hexViewingKey = 'AB34567890FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF';
      log.debug(`hexViewingKey = ${hexViewingKey}`);

      // Expect the promise to reject with an error
      await expect(indexerWsClient.openWalletSession(hexViewingKey)).rejects.toThrow();
    });

    /**
     * Opening a session with an invalid viewing key returns an error
     *
     * @given an invalid viewing key
     * @when we open a session with that viewing key
     * @then Indexer should return an error
     */
    test('should return an error, given an invalid viewing key', async () => {
      const generatedViewingKey = generateSyntheticViewingKey('dev1');
      log.debug(`generatedViewingKey = ${generatedViewingKey}`);

      // Expect the promise to reject with an error
      await expect(indexerWsClient.openWalletSession(generatedViewingKey)).rejects.toThrow();
    });

    /**
     * Opening a session with a valid viewing key meant for a different network returns an error
     *
     * @given a valid viewing key meant for a different network
     * @when we open a session with that viewing key
     * @then Indexer should return an error
     */
    test('should return an error, given a valid viewing key meant for a different network', async (_ctx: TestContext) => {
      log.info(`Seed for viewing key = ${randomSeed}`);

      // Get all the ledger network ids
      const networkIds = env.getAllEnvironmentNames();
      for (const networkId of networkIds) {
        log.debug(`networkId = ${networkId}`);
        const viewingKey = await toolkit.showViewingKey(randomSeed, networkId);
        log.debug(`viewingKey = ${viewingKey}`);
        if (networkId === env.getNetworkId().toLowerCase()) {
          continue;
        }
        await expect
          .soft(indexerWsClient.openWalletSession(viewingKey))
          .rejects.toThrow(/expected HRP.*but was/);
      }
    });
  });

  describe('closing a session with session ID', async () => {
    /**
     * Closing a session with a valid session ID terminates the session successfully
     *
     * @given a valid session ID obtained from opening a wallet session
     * @when we close the session with that session ID
     * @then Indexer should terminate the session successfully
     */
    test('should terminate the session successfully, given a valid session ID', async () => {
      // Gets the viewing key for the random seed using toolkit
      const viewingKey = await toolkit.showViewingKey(randomSeed);
      log.debug(`viewingKey = ${viewingKey}`);

      const sessionId = await indexerWsClient.openWalletSession(viewingKey);

      return indexerWsClient
        .closeWalletSession(sessionId)
        .then((message: GraphQLCloseSessionMessage) => {
          log.debug(`Received message = ${JSON.stringify(message, null, 2)}`);
          expect(message.payload.data.disconnect).toBeDefined();
        })
        .catch((error) => {
          log.error(error);
          throw new Error(error);
        });
    });

    /**
     * Closing a session with an invalid session ID returns an error
     *
     * @given an invalid session ID
     * @when we attempt to close a session with that session ID
     * @then Indexer should return an error
     */
    test('should return an error, given an invalid session ID', async () => {
      const sessionId = 'invalid-session-id';
      log.debug(`sessionId = ${sessionId}`);

      await expect
        .soft(indexerWsClient.closeWalletSession(sessionId))
        .rejects.toThrow(/Unexpected payload in disconnect response/);
    });
  });

  describe('a subscription to wallet updates providing viewing key only', async () => {
    /**
     * Subscribing to wallet updates with a valid viewing key streams wallet events
     *
     * @given a valid viewing key and an open wallet session
     * @when we subscribe to shielded transaction events for that session
     * @then Indexer should stream wallet events starting from the beginning
     * @and we should receive at least one event
     */
    test('should stream wallet events starting from the beginning, given there are relevant transactions', async () => {
      // Seed with transaction from which we get viewing key
      const seedWithTransactions = dataProvider.getFundingSeed();
      const viewingKey = await toolkit.showViewingKey(seedWithTransactions);
      log.debug(`viewingKey = ${viewingKey}`);

      const sessionId: string = await indexerWsClient.openWalletSession(viewingKey);

      const receivedEvents: ShieldedTxSubscriptionResponse[] = [];
      const shieldedTxSubscriptionHandler: SubscriptionHandlers<ShieldedTxSubscriptionResponse> = {
        next: (payload) => {
          log.debug(`Received data:\n${JSON.stringify(payload)}`);
          receivedEvents.push(payload);
        },
        complete: () => {
          log.debug('Completed sent from Indexer');
        },
      };

      const unsubscribe = indexerWsClient.subscribeToShieldedTransactionEvents(
        shieldedTxSubscriptionHandler,
        sessionId,
      );

      await new Promise((res) => setTimeout(res, 2000));

      unsubscribe();

      expect(receivedEvents.length).toBeGreaterThanOrEqual(1);
      receivedEvents.forEach((event) => {
        expect(event).toBeSuccess();
      });
    });

    /**
     * Validates that all streamed shielded transaction events conform to the expected schema.
     *
     * @given a valid viewing key and an open wallet session
     * @when shielded transaction events are streamed from the indexer
     * @then each received event should match the ShieldedTransactionEventSchema definition
     */
    test('should stream shielded transaction events adhering to the expected schema', async () => {
      // Seed with transaction from which we get viewing key
      const seedWithTransactions = dataProvider.getFundingSeed();
      const viewingKey = await toolkit.showViewingKey(seedWithTransactions);
      log.debug(`viewingKey = ${viewingKey}`);

      const sessionId: string = await indexerWsClient.openWalletSession(viewingKey);

      const receivedEvents: ShieldedTxSubscriptionResponse[] = [];
      const shieldedTxSubscriptionHandler: SubscriptionHandlers<ShieldedTxSubscriptionResponse> = {
        next: (payload) => {
          log.debug(`Received data:\n${JSON.stringify(payload)}`);
          receivedEvents.push(payload);
        },
        complete: () => {
          log.debug('Completed sent from Indexer');
        },
      };

      const unsubscribe = indexerWsClient.subscribeToShieldedTransactionEvents(
        shieldedTxSubscriptionHandler,
        sessionId,
      );
      await new Promise((res) => setTimeout(res, 3000));
      unsubscribe();

      // Filter out successful events
      receivedEvents
        .filter((msg) => msg?.data?.shieldedTransactions)
        .forEach((msg) => {
          expect.soft(msg).toBeSuccess();
          const eventData = msg.data?.shieldedTransactions;
          const parsed = ShieldedTransactionEventSchema.safeParse(eventData);

          expect(
            parsed.success,
            `Shielded transaction event schema validation failed: ${JSON.stringify(
              parsed.error?.format(),
              null,
              2,
            )}`,
          ).toBe(true);
        });
    });

    /**
     * The shielded transaction subscription provides progress events with highestZswapEndIndex.
     * A wallet can use this value to request a collapsed Merkle tree update via the
     * zswapMerkleTreeCollapsedUpdate query, mirroring the real wallet sync flow.
     *
     * @given a valid viewing key and an open wallet session
     * @when we receive a ShieldedTransactionsProgress event with highestZswapEndIndex
     * @then using that endIndex in zswapMerkleTreeCollapsedUpdate should return a valid result
     */
    test('should be able to use highestZswapEndIndex from progress event in collapsed update query', async () => {
      const seedWithTransactions = dataProvider.getFundingSeed();
      const viewingKey = await toolkit.showViewingKey(seedWithTransactions);

      const sessionId: string = await indexerWsClient.openWalletSession(viewingKey);

      // Collect events until we get a ShieldedTransactionsProgress. The 15s ceiling
      // relies on the first progress update being emitted immediately on subscribe,
      // which still holds under the idle backoff introduced in indexer 4.4.0.
      const highestZswapEndIndex = await new Promise<number>((resolve, reject) => {
        const timeout = setTimeout(() => {
          reject(new Error('Timed out waiting for ShieldedTransactionsProgress event'));
        }, 15_000);

        const unsubscribe = indexerWsClient.subscribeToShieldedTransactionEvents(
          {
            next: (payload) => {
              const event = payload.data?.shieldedTransactions;
              if (event?.__typename === 'ShieldedTransactionsProgress') {
                log.debug(`Received progress event: ${JSON.stringify(event)}`);
                clearTimeout(timeout);
                unsubscribe();
                resolve(event.highestZswapEndIndex);
              }
            },
          },
          sessionId,
        );
      });

      log.debug(`highestZswapEndIndex from progress event = ${highestZswapEndIndex}`);
      expect(highestZswapEndIndex).toBeGreaterThan(0);

      // highestZswapEndIndex is exclusive, so the collapsed update query needs (highestZswapEndIndex - 1)
      const indexerHttpClient = new IndexerHttpClient();
      const endIndex = highestZswapEndIndex - 1;
      log.debug(`Querying collapsed update with startIndex=0, endIndex=${endIndex}`);
      const response = await indexerHttpClient.getZswapMerkleTreeCollapsedUpdate(0, endIndex);

      expect(response).toBeSuccess();
      expect(response.data?.zswapMerkleTreeCollapsedUpdate).toBeDefined();

      const collapsedUpdate = response.data!.zswapMerkleTreeCollapsedUpdate;
      expect(collapsedUpdate.startIndex).toBe(0);
      expect(collapsedUpdate.endIndex).toBe(endIndex);

      log.debug('Validating collapsed update schema');
      const parsed = MerkleTreeCollapsedUpdateSchema.safeParse(collapsedUpdate);
      expect(
        parsed.success,
        `Collapsed update schema validation failed ${JSON.stringify(parsed.error, null, 2)}`,
      ).toBe(true);

      await indexerWsClient.closeWalletSession(sessionId);
    }, 30_000);

    /**
     * Ensures that a shielded transaction subscription cannot use a session ID
     * after the wallet session has been disconnected.
     *
     * @given a valid viewing key and an open wallet session
     * @when the wallet session is disconnected
     * @then subscriptions using the old session ID should fail with "unknown or expired session ID"
     */
    test('should reject shieldedTransactions subscription when using expired session ID', async () => {
      const seedWithTransactions = dataProvider.getFundingSeed();
      const viewingKey = await toolkit.showViewingKey(seedWithTransactions);

      const sessionId = await indexerWsClient.openWalletSession(viewingKey);

      // Confirm the session is alive by waiting for the first RelevantTransaction event.
      // We don't need the full transaction history — one relevant event is sufficient
      // proof that the session is working.
      const beforeLogoutEvents: ShieldedTxSubscriptionResponse[] = [];
      await new Promise<void>((resolve, reject) => {
        const timeout = setTimeout(() => {
          reject(
            new Error(
              `Timed out waiting for a shielded event before logout. ` +
                `Received ${beforeLogoutEvents.length} event(s): ${JSON.stringify(beforeLogoutEvents)}`,
            ),
          );
        }, 30_000);
        const unsubscribe = indexerWsClient.subscribeToShieldedTransactionEvents(
          {
            next: (payload) => {
              beforeLogoutEvents.push(payload);
              log.debug(`Received event before logout: ${JSON.stringify(payload)}`);
              if (
                payload.data?.shieldedTransactions?.__typename !== 'ShieldedTransactionsProgress'
              ) {
                clearTimeout(timeout);
                unsubscribe();
                resolve();
              }
            },
          },
          sessionId,
        );
      });

      await indexerWsClient.closeWalletSession(sessionId);

      // After logout, subscribing with the expired session ID must produce an error event.
      const afterLogoutEvents: ShieldedTxSubscriptionResponse[] = [];
      await new Promise<void>((resolve, reject) => {
        const timeout = setTimeout(() => {
          reject(
            new Error(
              `Timed out waiting for expired-session error event. ` +
                `Received ${afterLogoutEvents.length} event(s): ${JSON.stringify(afterLogoutEvents)}`,
            ),
          );
        }, 10_000);
        const checkForExpiredSession = (payload: ShieldedTxSubscriptionResponse) => {
          afterLogoutEvents.push(payload);
          log.debug(`Received event after logout: ${JSON.stringify(payload)}`);
          if (payload.errors?.some((e) => e.message.includes('unknown or expired session ID'))) {
            clearTimeout(timeout);
            unsubscribe();
            resolve();
          }
        };
        const unsubscribe = indexerWsClient.subscribeToShieldedTransactionEvents(
          {
            next: checkForExpiredSession,
            error: (err) => {
              checkForExpiredSession(buildErrorPayload<ShieldedTxSubscriptionResponse>(err));
            },
          },
          sessionId,
        );
      });
    }, 45_000);
  });
});
