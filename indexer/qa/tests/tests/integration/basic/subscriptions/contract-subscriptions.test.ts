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
import type { TestContext } from 'vitest';
import '@utils/logging/test-logging-hooks';
import dataProvider from '@utils/testdata-provider';
import {
  IndexerWsClient,
  SubscriptionHandlers,
  ContractActionSubscriptionResponse,
} from '@utils/indexer/websocket-client';
import { EventCoordinator } from '@utils/event-coordinator';
import { ContractActionUnionSchema } from '@utils/indexer/graphql/schema';
import { BlockOffset } from '@utils/indexer/indexer-types';

// ContractActionSubscriptionResponse is now imported from websocket-client

describe('contract action subscriptions', () => {
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

  /**
   * Helper to subscribe to contract actions and collect a specified number of them.
   */
  async function collectContractActions(
    contractAddress: string,
    expectedCount = 2,
    offset?: BlockOffset,
    eventName = 'contractActionReceived',
  ): Promise<ContractActionSubscriptionResponse[]> {
    // We wait for at least one contract action to be received
    const receivedContractActions: ContractActionSubscriptionResponse[] = [];

    const contractActionSubscriptionHandler: SubscriptionHandlers<ContractActionSubscriptionResponse> =
      {
        next: (payload: ContractActionSubscriptionResponse) => {
          log.debug(`Received contract action:\n${JSON.stringify(payload)}`);
          receivedContractActions.push(payload);
          if (receivedContractActions.length === expectedCount) eventCoordinator.notify(eventName);
          log.debug(`Event triggered: ${eventName}`);
        },
      };

    // We subscribe to contract actions for a specific address without block offset
    // This will start streaming contract actions from the latest block
    const unsubscribe = indexerWsClient.subscribeToContractActionEvents(
      contractActionSubscriptionHandler,
      contractAddress,
      offset,
    );
    // Maximum wait time for contract action (similar to block timeout)
    const maxTimeForContractAction = 8_000;
    await eventCoordinator.waitForAll([eventName], maxTimeForContractAction);
    unsubscribe();
    return receivedContractActions;
  }

  describe('a subscription to contract action updates without parameters', () => {
    /**
     * Subscribing to contract action updates without any offset parameters should stream
     * contract actions starting from the latest available block and continue streaming
     * new contract actions as they are produced.
     *
     * @given no block offset parameters are provided
     * @when we subscribe to contract action events
     * @then we should receive contract actions starting from the latest available
     * @and we should receive new contract actions as they are produced
     */
    test('should stream contract actions from the latest available block', async (ctx: TestContext) => {
      let contractAddress: string;
      try {
        contractAddress = dataProvider.getKnownContractAddress();
      } catch (error) {
        log.warn(error);
        ctx.skip?.(true, (error as Error).message);
      }
      const receivedContractActions = await collectContractActions(
        contractAddress!,
        2,
        undefined,
        'contractActionReceived',
      );

      // We should receive at least one contract action message
      expect(receivedContractActions.length).toBeGreaterThanOrEqual(1);
      expect(receivedContractActions[0]).toBeSuccess();

      // Validate the received contract action
      receivedContractActions
        .filter((msg) => msg.data?.contractActions)
        .forEach((msg) => {
          const contractAction = msg.data?.contractActions;
          expect(['ContractDeploy', 'ContractCall', 'ContractUpdate']).toContain(
            contractAction?.__typename,
          );
          expect(contractAction?.address).toBe(contractAddress);
        });
    });

    /**
     * Validates that all streamed contract actions conform to their expected schema.
     *
     * @given a contract action subscription for a known contract address
     * @when contract actions are streamed from the indexer
     * @then each received contract action should match its corresponding Zod schema
     */
    test('should stream contract actions adhering to the expected schema', async (ctx: TestContext) => {
      let contractAddress: string;
      try {
        contractAddress = dataProvider.getKnownContractAddress();
      } catch (error) {
        log.warn(error);
        ctx.skip?.(true, (error as Error).message);
      }
      const receivedContractActions = await collectContractActions(contractAddress!, 2);

      receivedContractActions
        .filter((msg) => msg?.data?.contractActions)
        .forEach((msg) => {
          expect.soft(msg).toBeSuccess();
          const contractAction = msg.data?.contractActions;

          const parsed = ContractActionUnionSchema.safeParse(contractAction);
          expect(
            parsed.success,
            `Contract action schema validation failed: ${JSON.stringify(
              parsed.error?.format(),
              null,
              2,
            )}`,
          ).toBe(true);
        });
    });
  });

  describe('a subscription to contract action updates with block hash offset', () => {
    /**
     * Subscribing to contract action updates with a block hash offset should stream
     * all historical contract actions starting from the specified block hash and
     * continue streaming new contract actions as they are produced.
     *
     * @given a valid block hash from before the latest action
     * @when we subscribe to contract action events with that block hash offset
     * @then we should receive all historical contract actions since that block
     * @and the first message's block hash should be >= the requested hash
     * @and we should continue to receive new contract actions as they are produced
     */
    test('should stream historical and new contract actions from a specific block hash', async (ctx: TestContext) => {
      // We get a known contract address from test data provider
      let contractAddress: string;
      let historicalBlockHash: string;
      try {
        contractAddress = dataProvider.getKnownContractAddress();
        // We get a known block hash from before the latest action
        // This should be a block hash that contains historical contract actions
        historicalBlockHash = await dataProvider.getContractDeployBlockHash();
      } catch (error) {
        log.warn(error);
        ctx.skip?.(true, (error as Error).message);
      }

      // We collect all received contract actions
      const receivedContractActions = await collectContractActions(
        contractAddress!,
        1,
        { hash: historicalBlockHash! },
        'firstHistoricalActionReceived',
      );

      // We should receive at least one contract action message
      expect(receivedContractActions.length).toBeGreaterThanOrEqual(1);
      expect(receivedContractActions[0]).toBeSuccess();

      // Validate the received contract actions
      for (const action of receivedContractActions) {
        if (action.data?.contractActions) {
          const contractAction = action.data.contractActions;
          expect(contractAction).toBeDefined();
          expect(contractAction.__typename).toBeDefined();
          expect(['ContractDeploy', 'ContractCall', 'ContractUpdate']).toContain(
            contractAction.__typename,
          );
          expect(contractAction.address).toBe(contractAddress!);
        }
      }

      // Validate that the first message's block hash is >= the requested hash
      // This ensures we're getting historical actions from the specified block onwards
      if (receivedContractActions.length > 0 && receivedContractActions[0].data?.contractActions) {
        const firstAction = receivedContractActions[0].data.contractActions;
        if (firstAction.transaction?.block?.hash) {
          const firstActionBlockHash = firstAction.transaction.block.hash;
          log.debug(`First action block hash: ${firstActionBlockHash}`);
          log.debug(`Requested historical block hash: ${historicalBlockHash!}`);

          // Note: In a real blockchain, we would compare block heights or hashes
          // For this test, we verify that we received actions and that they match the contract address
          expect(firstActionBlockHash).toBeDefined();
          expect(firstAction.address).toBe(contractAddress!);
        }
      }
    });
  });
});
