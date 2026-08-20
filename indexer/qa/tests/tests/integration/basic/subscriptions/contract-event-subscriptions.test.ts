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
import { env } from 'environment/model';
import type { TestContext } from 'vitest';
import '@utils/logging/test-logging-hooks';
import { IndexerWsClient } from '@utils/indexer/websocket-client';
import { extractSubscriptionErrorMessage } from '@utils/indexer/subscription-error';
import {
  contractEventsSurfacePresent,
  indexedContractFieldsOf,
} from '@utils/indexer/contract-events-support';
import { ContractEventUnionSchema } from '@utils/indexer/graphql/schema';
import { IndexerHttpClient } from '@utils/indexer/http-client';
import type { ContractEvent } from '@utils/indexer/indexer-types';
import dataProvider, { type EventEmittingContractInfo } from '@utils/testdata-provider';

const httpClient = new IndexerHttpClient();

describe('contract event subscription', () => {
  let surfacePresent = false;
  let wsClient: IndexerWsClient;

  beforeAll(async () => {
    surfacePresent = await contractEventsSurfacePresent();
    if (!surfacePresent) {
      log.warn(
        `Contract events surface absent on ${env.getCurrentEnvironmentName()}; ` +
          `contract event subscription tests will be skipped.`,
      );
    }
  }, 30_000);

  beforeEach(async () => {
    if (!surfacePresent) return;
    wsClient = new IndexerWsClient();
    await wsClient.connectionInit();
  }, 30_000);

  afterEach(async () => {
    if (!surfacePresent) return;
    await wsClient.connectionClose();
  });

  describe('a contract events subscription for an address with no emitted events', () => {
    /**
     * A bounded subscription for an idle address streams nothing and terminates.
     *
     * @given a valid-format contract address that has emitted no events and a
     *        toBlock the chain has already passed
     * @when a contract events subscription is opened with that bounded filter
     * @then the stream completes via the toBlock terminator without delivering events
     */
    test('should complete via the toBlock terminator without delivering events', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Subscription', 'ContractEvents'] };
      if (!surfacePresent) return ctx.skip?.(true, 'contract events surface not present');

      const blockResponse = await httpClient.getLatestBlock();
      expect(blockResponse).toBeSuccess();
      const toBlock = Math.min(blockResponse.data!.block.height, 5);
      const contractAddress = dataProvider.getNonExistingContractAddress();

      const settled = await new Promise<{ completed: boolean; eventCount: number }>(
        (resolve, reject) => {
          let eventCount = 0;
          const timeout = setTimeout(() => {
            subscription.unsubscribe();
            reject(new Error('Timed out waiting for the toBlock terminator'));
          }, 20_000);

          const subscription = wsClient.subscribeToContractEvents(
            {
              next: () => {
                eventCount++;
              },
              error: (error) => {
                clearTimeout(timeout);
                subscription.unsubscribe();
                reject(new Error(`Subscription error: ${JSON.stringify(error)}`));
              },
              complete: () => {
                clearTimeout(timeout);
                resolve({ completed: true, eventCount });
              },
            },
            { contractAddress, fromBlock: 0, toBlock },
            0,
          );
        },
      );

      expect(settled.completed).toBe(true);
      expect(settled.eventCount).toBe(0);
    }, 30_000);
  });

  describe('a contract events subscription with an invalid filter', () => {
    /**
     * An empty contractAddress is rejected by the subscription.
     *
     * @given a contract events filter whose contractAddress is the empty string
     * @when a contract events subscription is opened with that filter
     * @then the subscription surfaces an error rather than streaming events
     */
    test('should return an error when the contract address is empty', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Subscription', 'ContractEvents', 'Negative'] };
      if (!surfacePresent) return ctx.skip?.(true, 'contract events surface not present');

      const settled = await new Promise<{ error: string | null; eventCount: number }>(
        (resolve, reject) => {
          let eventCount = 0;
          const timeout = setTimeout(() => {
            subscription.unsubscribe();
            // Validation errors are immediate; a 10s silence is a real problem,
            // so fail loudly with a timeout message rather than resolving
            // `error: null` and tripping the assertion with a misleading value.
            reject(
              new Error('Timed out after 10s waiting for the empty-address subscription to error'),
            );
          }, 10_000);

          const subscription = wsClient.subscribeToContractEvents(
            {
              next: () => {
                eventCount++;
              },
              error: (error) => {
                clearTimeout(timeout);
                subscription.unsubscribe();
                resolve({ error: extractSubscriptionErrorMessage(error), eventCount });
              },
              complete: () => {
                clearTimeout(timeout);
                resolve({ error: null, eventCount });
              },
            },
            { contractAddress: '' },
            0,
          );
        },
      );

      expect(settled.error).not.toBeNull();
      expect(settled.eventCount).toBe(0);
    }, 20_000);

    /**
     * An unknown field prefix field name is rejected by the subscription.
     *
     * @given a contract events filter with a fieldPrefixes entry whose fieldName
     *        "bogus" is not an indexable contract field
     * @when a contract events subscription is opened with that filter
     * @then the subscription surfaces an error rather than streaming events
     */
    test('should return an error for an unknown field prefix field name', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Subscription', 'ContractEvents', 'Negative'] };
      if (!surfacePresent) return ctx.skip?.(true, 'contract events surface not present');

      const settled = await new Promise<{ error: string | null; eventCount: number }>(
        (resolve, reject) => {
          let eventCount = 0;
          const timeout = setTimeout(() => {
            subscription.unsubscribe();
            // Validation errors are immediate; a 10s silence is a real problem,
            // so fail loudly rather than resolving `error: null` and tripping
            // the assertion with a misleading value.
            reject(
              new Error(
                'Timed out after 10s waiting for the unknown-field-name subscription to error',
              ),
            );
          }, 10_000);

          const subscription = wsClient.subscribeToContractEvents(
            {
              next: () => {
                eventCount++;
              },
              error: (error) => {
                clearTimeout(timeout);
                subscription.unsubscribe();
                resolve({ error: extractSubscriptionErrorMessage(error), eventCount });
              },
              complete: () => {
                clearTimeout(timeout);
                resolve({ error: null, eventCount });
              },
            },
            {
              contractAddress: dataProvider.getNonExistingContractAddress(),
              fieldPrefixes: [{ fieldName: 'bogus', prefix: '' }],
            },
            0,
          );
        },
      );

      expect(settled.error).not.toBeNull();
      expect(settled.eventCount).toBe(0);
    }, 20_000);
  });

  describe('a contract events subscription for a contract with emitted events', () => {
    /**
     * Streamed events conform to the contract event schema and belong to the contract.
     *
     * Skipped until an event-emitting contract fixture is configured for the
     * environment (see testdata-provider.getEventEmittingContracts); the
     * emit-bearing toolchain path is tracked by midnight-indexer#1163.
     *
     * @given a contract known to have emitted public contract events
     * @when a contract events subscription is opened for that contract from the
     *       start of the chain and bounded by the latest block
     * @then the stream delivers at least one event, each conforming to the
     *       contract event schema and reporting the subscribed contract address
     */
    test('should stream events conforming to the contract event schema', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Subscription', 'ContractEvents'] };
      if (!surfacePresent) return ctx.skip?.(true, 'contract events surface not present');

      let contracts: EventEmittingContractInfo[];
      try {
        contracts = dataProvider.getEventEmittingContracts();
      } catch (error) {
        log.warn(error);
        return ctx.skip?.(true, (error as Error).message);
      }

      const blockResponse = await httpClient.getLatestBlock();
      expect(blockResponse).toBeSuccess();
      const toBlock = blockResponse.data!.block.height;
      const contractAddress = contracts[0]['contract-address'];

      const received: ContractEvent[] = [];
      await new Promise<void>((resolve, reject) => {
        const timeout = setTimeout(() => {
          subscription.unsubscribe();
          reject(
            new Error(
              `Timed out after 20s waiting for a streamed contract event for ${contractAddress}`,
            ),
          );
        }, 20_000);

        const subscription = wsClient.subscribeToContractEvents(
          {
            next: (payload) => {
              const event = payload.data?.contractEvents;
              if (event) received.push(event);
              // Resolve as soon as an event streams, rather than waiting for the
              // `toBlock` terminator: the terminator's timing is exercised by the
              // idle-address test, and depending on it here would turn a healthy
              // stream into a flaky empty result whenever `complete` lands slower
              // than the timeout. Late in-flight events are validated too.
              if (received.length > 0) {
                clearTimeout(timeout);
                subscription.unsubscribe();
                resolve();
              }
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
          { contractAddress, fromBlock: 0, toBlock },
          0,
        );
      });

      expect(received.length).toBeGreaterThan(0);
      for (const event of received) {
        const parsed = ContractEventUnionSchema.safeParse(event);
        expect(
          parsed.success,
          `Contract event schema validation failed: ${JSON.stringify(parsed.error, null, 2)}`,
        ).toBe(true);
        expect(event.contractAddress).toBe(contractAddress);
      }
    }, 30_000);

    /**
     * A field-prefix-filtered subscription streams only events carrying the field.
     *
     * Skipped until an event-emitting contract fixture is configured for the
     * environment (see testdata-provider.getEventEmittingContracts); the
     * emit-bearing toolchain path is tracked by midnight-indexer#1163.
     *
     * @given a contract that emitted an event carrying an indexed field
     * @when a contract events subscription bounded by the latest block is opened
     *       filtered by that field name with an empty prefix
     * @then the stream completes via the toBlock terminator having delivered at
     *       least one event, each carrying the filtered field
     */
    test('should stream only events carrying the filtered field', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Subscription', 'ContractEvents'] };
      if (!surfacePresent) return ctx.skip?.(true, 'contract events surface not present');

      let contracts: EventEmittingContractInfo[];
      try {
        contracts = dataProvider.getEventEmittingContracts();
      } catch (error) {
        log.warn(error);
        return ctx.skip?.(true, (error as Error).message);
      }

      // Pick, via the query surface, a fixture contract and a field name one of
      // its events actually carries, so the subscription is expected to stream.
      let contractAddress: string | undefined;
      let fieldName: string | undefined;
      for (const contract of contracts) {
        const response = await httpClient.getContractEvents({
          contractAddress: contract['contract-address'],
        });
        expect(response).toBeSuccess();
        const fields = (response.data?.contractEvents ?? []).flatMap(indexedContractFieldsOf);
        if (fields.length > 0) {
          contractAddress = contract['contract-address'];
          fieldName = fields[0].fieldName;
          break;
        }
      }
      if (!contractAddress || !fieldName) {
        return ctx.skip?.(true, 'no event-emitting contract fixture exposes an indexed field');
      }

      const blockResponse = await httpClient.getLatestBlock();
      expect(blockResponse).toBeSuccess();
      const toBlock = blockResponse.data!.block.height;

      const received: ContractEvent[] = [];
      await new Promise<void>((resolve, reject) => {
        const timeout = setTimeout(() => {
          subscription.unsubscribe();
          reject(
            new Error(
              `Timed out after 20s waiting for the ${fieldName}-filtered stream to complete`,
            ),
          );
        }, 20_000);

        const subscription = wsClient.subscribeToContractEvents(
          {
            next: (payload) => {
              const event = payload.data?.contractEvents;
              if (event) received.push(event);
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
          {
            contractAddress,
            fieldPrefixes: [{ fieldName, prefix: '' }],
            fromBlock: 0,
            toBlock,
          },
          0,
        );
      });

      expect(received.length).toBeGreaterThan(0);
      for (const event of received) {
        expect(event.contractAddress).toBe(contractAddress);
        expect(
          indexedContractFieldsOf(event).map((field) => field.fieldName),
          `streamed event ${event.id} does not carry "${fieldName}"`,
        ).toContain(fieldName);
      }
    }, 30_000);
  });
});
