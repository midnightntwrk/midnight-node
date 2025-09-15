// This file is part of midnightntwrk/midnight-indexer
// Copyright (C) 2025 Midnight Foundation
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
import dataProvider from 'utils/testdata-provider';
import { GraphQLError } from 'graphql/error/GraphQLError';
import { encodeBech32mWithPrefix, decodeBech32m } from 'utils/bech32-codec';
import {
  IndexerWsClient,
  SubscriptionHandlers,
  UnshieldedTransactionSubscriptionParams,
  UnshieldedTxSubscriptionResponse,
} from '@utils/indexer/websocket-client';
import type {
  UnshieldedTransaction,
  UnshieldedTransactionEvent,
  UnshieldedTransactionsProgress,
  UnshieldedUtxo,
} from '@utils/indexer/indexer-types';

let indexerWsClient: IndexerWsClient;

/**
 * Utility function to subscribe to unshielded transaction events by address and/or transaction id.
 * This is to help to reuse the login and the handler in multiple places.
 *
 * @param subscriptionParams - The parameters for the subscription.
 * @param stopCondition - The condition to stop the subscription.
 * @param timeout - The timeout for the subscription.
 * @returns The received unshielded transactions.
 */
async function subscribeToUnshieldedTransactionEvents(
  subscriptionParams: UnshieldedTransactionSubscriptionParams,
  stopCondition: (message: UnshieldedTxSubscriptionResponse[]) => boolean,
  timeout: number = 500,
): Promise<UnshieldedTxSubscriptionResponse[]> {
  const receivedUnshieldedTransactions: UnshieldedTxSubscriptionResponse[] = [];

  let stopListening: () => void;
  let completePromiseResolver: () => void;

  const stopConditionPromise = new Promise<void>((resolve) => {
    stopListening = resolve;
  });

  const timeoutPromise = new Promise<void>((resolve) => {
    setTimeout(resolve, timeout);
  });

  const completePromise = new Promise<void>((resolve) => {
    completePromiseResolver = resolve;
  });

  const unshieldedTransactionSubscriptionHandler: SubscriptionHandlers<UnshieldedTxSubscriptionResponse> =
    {
      next: (payload) => {
        log.debug('Received data:\n', JSON.stringify(payload, null, 2));
        receivedUnshieldedTransactions.push(payload);
        if (stopCondition(receivedUnshieldedTransactions)) {
          stopListening();
        }
      },
      complete: () => {
        log.debug('Complete message sent from Indexer');
        completePromiseResolver();
      },
    };

  const unscribe = indexerWsClient.subscribeToUnshieldedTransactionEvents(
    unshieldedTransactionSubscriptionHandler,
    subscriptionParams,
  );

  // Once subscribed, we will wait for either the stop condition to be met,
  // the timeout to be reached or the complete message sent from the indexer
  // is received before unscribing from transaction events
  await Promise.race([stopConditionPromise, timeoutPromise, completePromise]);

  unscribe();

  return receivedUnshieldedTransactions;
}

// TODO: rename to unshielded transaction subscriptions for all the occurrences
// and update the xray tests
describe('unshielded utxo subscriptions', async () => {
  beforeAll(async () => {
    indexerWsClient = new IndexerWsClient();
    await indexerWsClient.connectionInit();
  });

  afterAll(async () => {
    await indexerWsClient.connectionClose();
  });

  describe('a subscription to unshielded utxo events by address', async () => {
    let messages: UnshieldedTxSubscriptionResponse[];

    /**
     * Subscribing to unshielded transaction events for an existing address should
     * stream all transactions that involve that address.
     *
     * @given an existing unshielded address that has transactions
     * @when we subscribe to unshielded transaction events by address only for that address
     * @then we should receive at least one transaction event
     * @and each transaction should involve the specified address
     */
    test('should stream unshielded utxo events related to that address', async () => {
      const unshieldedAddress = dataProvider.getUnshieldedAddress('existing');
      messages = await subscribeToUnshieldedTransactionEvents(
        { address: unshieldedAddress },
        (messages) => messages.length >= 10,
        500,
      );

      expect(messages.length).toBeGreaterThanOrEqual(1);
      messages.forEach((transaction) => {
        log.info('transaction', JSON.stringify(transaction, null, 2));
      });
    });

    /**
     * Subscribing to unshielded transaction events should provide a complete
     * stream up to the highest transaction ID and include a progress message.
     *
     * @given an existing unshielded address with multiple transactions
     * @when we subscribe to unshielded transaction events for that address
     * @then we should receive multiple transaction events
     * @and we should receive a progress message with the highest transaction ID
     * @and the highest transaction ID in events should match the progress message
     * @and we should receive at least 2 messages (transactions + progress)
     */
    test('should stream unshielded transaction events up to highest transaction id', async () => {
      const unshieldedAddress = dataProvider.getUnshieldedAddress('existing');
      messages = await subscribeToUnshieldedTransactionEvents(
        { address: unshieldedAddress },
        (messages) => messages.length >= 10,
        500,
      );

      messages.forEach((message) => {
        expect(
          message.errors,
          `Received unexpected error message: ${JSON.stringify(message.errors)}`,
        ).toBeUndefined();
        expect(message.data).toBeDefined();
      });

      let highestTransactionId = -1; // Reported by the indexer via the progress message
      let highestFoundTransactionId = -1; // Highest transaction id found in the events
      let foundTransactionProgressMessage = false;

      messages.forEach((message) => {
        const transactionEvent: UnshieldedTransactionEvent = message.data
          ?.unshieldedTransactions as UnshieldedTransactionEvent;
        if (transactionEvent.__typename === 'UnshieldedTransaction') {
          highestFoundTransactionId = Math.max(
            highestFoundTransactionId,
            transactionEvent.transaction.id,
          );
        } else if (transactionEvent.__typename === 'UnshieldedTransactionsProgress') {
          highestTransactionId = transactionEvent.highestTransactionId;
          foundTransactionProgressMessage = true;
        }
      });

      // We want a transaction progress message and at least one transaction event
      expect(messages.length).toBeGreaterThanOrEqual(2);

      // We want the transaction progress message to be found
      expect(foundTransactionProgressMessage).toBe(true);

      // We want the highest transaction id to be the same as the one reported by the indexer
      expect(highestFoundTransactionId).toBe(highestTransactionId);
    });

    test.skip('IMPLEMENT ME: should stream unshielded utxo events that adhere to the expected schema', async () => {
      // TODO: implement the following test that checks the schema of the unshielded transaction events
    });

    /**
     * Subscribing to unshielded transaction events for an address without transactions
     * should return only a progress message with highest transaction ID of 0.
     *
     * @given an unshielded address that has no transactions
     * @when we subscribe to unshielded transaction events for that address
     * @then we should receive exactly one UnshieldedTransactionsProgress message
     * @and the highestTransactionId should be 0
     */
    test('should only return a transaction progress message with highest transaction = 0, given that address does not have transactions', async () => {
      const unshieldedAddress = dataProvider.getUnshieldedAddress('non-existing');
      messages = await subscribeToUnshieldedTransactionEvents(
        { address: unshieldedAddress },
        (messages) => messages.length >= 10,
        500,
      );

      // We expect exactly one (non-error) message ...
      expect(messages.length).toBe(1);
      expect(
        messages[0].errors,
        `Received unexpected error message: ${JSON.stringify(messages[0].errors)}`,
      ).toBeUndefined();
      expect(messages[0].data).toBeDefined();

      // ... to be of UnshieldedTransactionsProgress type ...
      const transactionEvent: UnshieldedTransactionEvent = messages[0].data
        ?.unshieldedTransactions as UnshieldedTransactionEvent;
      expect(transactionEvent.__typename).toBe('UnshieldedTransactionsProgress');
      const transactionProgressEvent = transactionEvent as UnshieldedTransactionsProgress;

      // ... with no transactions present for this address -> highestTransactionId must be 0
      expect(transactionProgressEvent.highestTransactionId).toBe(0);
    });

    /**
     * Subscribing to unshielded transaction events with an invalid hex format address
     * should return an error message indicating the address is invalid.
     *
     * @given an address in hex format that is not valid for unshielded addresses
     * @when we subscribe to unshielded transaction events for that address
     * @then we should receive exactly one error message that indicates invalid address format
     */
    test('should return an error message, given the address provided is in hex format', async () => {
      // A random address in hex format that should be rejected
      const unshieldedAddress = '2c07534534b0c79727d87808a0ed03213d2b8072c58077c02bd71c8436170288';

      messages = await subscribeToUnshieldedTransactionEvents(
        { address: unshieldedAddress },
        (messages) => messages.length >= 10,
        500,
      );

      expect(messages.length).toBe(1);
      const msg = messages[0];
      expect(msg.data).toBeDefined();
      expect(msg.data).toBeNull();
      expect(msg.errors).toBeDefined();
      // TODO: soft assert on the error message
      expect((msg.errors as GraphQLError[])[0].message).toMatch(/^invalid address/);
    });

    /**
     * Subscribing to unshielded transaction events with a address that is not meant for that
     * specific network should return an error message indicating the address is invalid.
     *
     * @given an address that is not meant for the network we are targeting
     * @when we subscribe to unshielded transaction events for that address
     * @then we should receive exactly one error message that indicates invalid address
     */
    test('should return an error message, given the address provided is for another network', async () => {
      // A random address in hex format that should be rejected
      const unshieldedAddressMapByNetwork: Record<string, string> = {
        Undeployed: 'mn_addr_undeployed1g9nr3mvjcey7ca8shcs5d4yjndcnmczf90rhv4nju7qqqlfg4ygs0t4ngm',
        Devnet: 'mn_addr_dev1g9nr3mvjcey7ca8shcs5d4yjndcnmczf90rhv4nju7qqqlfg4ygsv7kard',
        Testnet: 'mn_addr_test1g9nr3mvjcey7ca8shcs5d4yjndcnmczf90rhv4nju7qqqlfg4ygs72dqyf',
      };

      let encodedAddress = encodeBech32mWithPrefix(
        unshieldedAddressMapByNetwork.Undeployed,
        'mn_addr_dev',
      );
      log.info(`Encoded address for devnet: ${encodedAddress}`);
      let decodedAddress = decodeBech32m(encodedAddress);
      log.info(`Decoded address for devnet: ${decodedAddress}`);

      encodedAddress = encodeBech32mWithPrefix(
        unshieldedAddressMapByNetwork.Undeployed,
        'mn_addr_test',
      );
      log.info(`Encoded address for testnet: ${encodedAddress}`);
      decodedAddress = decodeBech32m(encodedAddress);
      log.info(`Decoded address for testnet: ${decodedAddress}`);

      const currentNetworkId = env.getNetworkId();
      const targetNetworks = { ...unshieldedAddressMapByNetwork };
      delete targetNetworks[currentNetworkId];

      for (const [targetNetwork, targetAddress] of Object.entries(targetNetworks)) {
        log.info(
          `Address ${targetAddress} targets network ${targetNetwork} is rejected on ${currentNetworkId}`,
        );

        messages = await subscribeToUnshieldedTransactionEvents(
          { address: targetAddress },
          (messages) => messages.length >= 10,
          500,
        );

        expect(messages.length).toBe(1);
        const msg = messages[0];
        expect(msg.data).toBeNull();
        expect(msg.errors).toBeDefined();
        expect(msg.errors?.[0].message).toMatch(/invalid address/);
        expect(msg.errors?.[0].message).toMatch(/network ID mismatch/);
      }
    });
  });

  describe('a subscription to unshielded transaction events by address and transaction id', async () => {
    /**
     * Subscribing to unshielded transaction events with transaction id = 0
     * should stream transactions starting from that ID and all the transactions
     * should be related to the provided address.
     *
     * @given an unshielded address that has transactions
     * @when we subscribe to unshielded transaction events with address and transaction id = 0
     * @then we should receive multiple transaction events
     * @and each transaction should have an ID greater than or equal to the starting ID
     * @and each transaction should involve the specified address in either spent or created UTXOs
     * @and we should receive a progress message with the highest transaction ID
     * @and the highest transaction ID in events should match the progress message
     */
    test('should return a stream of transactions containing that address, starting from transaction id = 0', async () => {
      const targetTransactionId = 0;
      const targetAddress = dataProvider.getUnshieldedAddress('existing');

      const messages = await subscribeToUnshieldedTransactionEvents(
        { address: targetAddress, transactionId: targetTransactionId },
        (messages) => messages[0].errors !== undefined,
        200,
      );

      expect(messages.length).toBeGreaterThanOrEqual(2);

      let highestTransactionId = -1; // Reported by the indexer via the progress message
      let highestFoundTransactionId = -1; // Hightest transaction id in all the transactions for the provided address
      let foundTransactionIds: number[] = []; // All the transactions for the provided address

      messages.forEach((message) => {
        const transactionEvent: UnshieldedTransactionEvent = message.data
          ?.unshieldedTransactions as UnshieldedTransactionEvent;

        if (transactionEvent.__typename === 'UnshieldedTransaction') {
          // Only messages of type UnshieldedTransaction are relevant to the address
          const isAnyUtxoRelevant = (utxos: UnshieldedUtxo[]) =>
            utxos.some((utxo) => utxo.owner === targetAddress);
          const foundAddressInCreatedUtxos = isAnyUtxoRelevant(transactionEvent.createdUtxos);
          const foundAddressInSpentUtxos = isAnyUtxoRelevant(transactionEvent.spentUtxos);

          // The address must be present in either the spent or created utxos
          // otherwise the indexer shouldn't have sent this message
          expect(foundAddressInCreatedUtxos || foundAddressInSpentUtxos).toBe(true);

          foundTransactionIds.push(transactionEvent.transaction.id);
        } else if (transactionEvent.__typename === 'UnshieldedTransactionsProgress') {
          highestTransactionId = transactionEvent.highestTransactionId;
        }
      });

      foundTransactionIds.forEach((id) => {
        expect(id).toBeGreaterThanOrEqual(targetTransactionId);
        highestFoundTransactionId = Math.max(highestFoundTransactionId, id);
      });
      expect(highestFoundTransactionId).toBe(highestTransactionId);
    });

    /**
     * Subscribing to unshielded transaction events with a negative transaction ID
     * should return an error message indicating the number is invalid.
     *
     * @given an existing unshielded address and a negative transaction ID
     * @when we subscribe to unshielded transaction events with address and negative transaction ID
     * @then we should receive exactly one message that contains an error indicating invalid number
     */
    test('should return an error message, given the transaction id provided is negative', async () => {
      // A random address in hex format that should be rejected
      const targetTransactionId = -1;
      const targetAddress = dataProvider.getUnshieldedAddress('existing');

      const messages = await subscribeToUnshieldedTransactionEvents(
        { address: targetAddress, transactionId: targetTransactionId },
        (messages) => messages.length >= 10,
        1000,
      );

      expect(messages.length).toBe(1);
      expect(messages[0].data).toBeNull();
      expect(messages[0].errors).toBeDefined();
      expect((messages[0].errors as GraphQLError[])[0].message).toMatch(/Invalid number/);
    });

    /**
     * Subscribing to unshielded transaction events with a transaction ID higher than
     * the current highest should return only a progress message without streaming transactions.
     *
     * @given an existing unshielded address and a transaction ID higher than current maximum
     * @when we subscribe to unshielded transaction events with address and high transaction ID
     * @then we should receive exactly one UnshieldedTransactionsProgress message
     * @and no transaction events should be streamed
     */
    test('should only return a transaction progress message without streaming transactions, given that the transaction id provided is bigger number', async () => {
      const targetTransactionId = 4294967296; // 2^32
      const targetAddress = dataProvider.getUnshieldedAddress('existing');

      const messages = await subscribeToUnshieldedTransactionEvents(
        { address: targetAddress, transactionId: targetTransactionId },
        (messages) => messages.length >= 10,
        1000,
      );

      // We expect exactly one (non-error) message ...
      expect(messages.length).toBe(1);
      expect(
        messages[0].errors,
        `Received unexpected error message: ${JSON.stringify(messages[0].errors)}`,
      ).toBeUndefined();
      expect(messages[0].data).toBeDefined();
      expect(messages[0].data).not.toBeNull();
      expect(messages[0].data?.unshieldedTransactions).toBeDefined();
      expect(messages[0].data?.unshieldedTransactions).not.toBeNull();

      // ... to be of UnshieldedTransactionsProgress type ...
      const transactionEvent: UnshieldedTransactionEvent = messages[0].data
        ?.unshieldedTransactions as UnshieldedTransactionEvent;
      expect(transactionEvent.__typename).toBe('UnshieldedTransactionsProgress');
      const transactionProgressEvent = transactionEvent as UnshieldedTransactionsProgress;
    });

    /**
     * Subscribing to unshielded transaction events with a specific transaction ID
     * should start streaming from that exact transaction ID onwards.
     *
     * @given an existing unshielded address and a known transaction ID
     * @when we subscribe to unshielded transaction events with address and that specific transaction ID
     * @then we should receive transaction events that start from the specified transaction ID
     */
    test('should start a transaction stream from the given transaction id', async () => {
      const targetTransactionId = 4294967296; // 2^32
      const targetAddress = dataProvider.getUnshieldedAddress('existing');

      let messages = await subscribeToUnshieldedTransactionEvents(
        { address: targetAddress },
        (messages) => messages.length >= 10,
        500,
      );

      let highestTransactionId = -1;
      let foundTransactionIds: number[] = [];

      // For each message, if it's a progress message, we update the highest transaction id
      // if it's a transaction event, we add the transaction id to the list of found transaction ids
      messages.forEach((msg) => {
        expect(
          msg.errors,
          `Received unexpected error message: ${JSON.stringify(msg.errors)}`,
        ).toBeUndefined();
        expect(msg.data).toBeDefined();
        expect(msg.data).not.toBeNull();
        expect(msg.data?.unshieldedTransactions).toBeDefined();
        expect(msg.data?.unshieldedTransactions).not.toBeNull();

        const transactionEvent: UnshieldedTransactionEvent = msg.data
          ?.unshieldedTransactions as UnshieldedTransactionEvent;

        if (transactionEvent.__typename === 'UnshieldedTransactionsProgress') {
          const transactionProgressEvent = transactionEvent as UnshieldedTransactionsProgress;
          highestTransactionId = transactionProgressEvent.highestTransactionId;
        } else if (transactionEvent.__typename === 'UnshieldedTransaction') {
          const unshieldedTransaction = transactionEvent as UnshieldedTransaction;
          foundTransactionIds.push(unshieldedTransaction.transaction.id);
        }
      });

      // Here we have processed a first stream for that address. Now we can subscribe using
      // any of the transaction ids and check if we get the expected transactions streamed
      // i.e. starting from the provided transaction id
      log.debug(`Found ${foundTransactionIds.length} transactions`);
      const randomTransactionId =
        foundTransactionIds[Math.floor(Math.random() * foundTransactionIds.length)];
      log.debug(`Random transaction id: ${randomTransactionId}`);
      messages = await subscribeToUnshieldedTransactionEvents(
        { address: targetAddress, transactionId: randomTransactionId },
        (messages) => false,
        1000,
      );

      expect(messages.length).toBeGreaterThanOrEqual(1);
      messages.forEach((msg) => {
        expect(msg.errors).toBeUndefined();
        expect(msg.data).toBeDefined();
        expect(msg.data).not.toBeNull();
        expect(msg.data?.unshieldedTransactions).toBeDefined();
        expect(msg.data?.unshieldedTransactions).not.toBeNull();

        const transactionEvent: UnshieldedTransactionEvent = msg.data
          ?.unshieldedTransactions as UnshieldedTransactionEvent;

        if (transactionEvent.__typename === 'UnshieldedTransaction') {
          const unshieldedTransaction = transactionEvent as UnshieldedTransaction;
          log.info(`Found transaction id: ${unshieldedTransaction.transaction.id}`);
          expect(unshieldedTransaction.transaction.id).toBeGreaterThanOrEqual(randomTransactionId);
        }
      });
    });

    /**
     * Should return an error message, given the unshielded address is provided in hex format
     *
     * @given an address in hex format
     * @when we subscribe to unshielded transaction events with that address
     * @then we should receive exactly one error message that indicates invalid address format
     */
    test('should return an error message, given the address is provided in hex format', async () => {
      const unshieldedAddress = '2c07534534b0c79727d87808a0ed03213d2b8072c58077c02bd71c8436170288';

      const messages = await subscribeToUnshieldedTransactionEvents(
        { address: unshieldedAddress, transactionId: 0 },
        (messages) => false,
        1000,
      );

      expect(messages.length).toBe(1);
      expect(messages[0].errors).toBeDefined();
      expect((messages[0].errors as GraphQLError[])[0].message).toMatch(/^invalid address/);
    });
  });
});
