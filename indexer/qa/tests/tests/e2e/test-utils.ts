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

import { BlockResponse, TransactionResponse } from '@utils/indexer/indexer-types';
import { IndexerHttpClient } from '@utils/indexer/http-client';
import { z } from 'zod';
import log from '@utils/logging/logger';
import { IndexerWsClient, UnshieldedTxSubscriptionResponse } from '@utils/indexer/websocket-client';
import { ToolkitWrapper, type ToolkitTransactionResult } from '@utils/toolkit/toolkit-wrapper';

export function retry<T>(
  fn: () => Promise<T>,
  condition: (result: T) => boolean,
  maxAttempts: number,
  delay: number,
): Promise<T> {
  return new Promise((resolve, reject) => {
    let attempts = 0;
    const execute = () => {
      attempts++;
      fn()
        .then((result) => {
          if (condition(result)) {
            resolve(result);
          } else if (attempts < maxAttempts) {
            setTimeout(execute, delay);
          } else {
            reject(new Error(`Condition not met after ${maxAttempts} attempts`));
          }
        })
        .catch((error) => {
          if (attempts < maxAttempts) {
            setTimeout(execute, delay);
          } else {
            reject(error);
          }
        });
    };
    execute();
  });
}

export function retrySimple<T>(
  fn: () => Promise<T | null>,
  maxAttempts = 10,
  delayMs = 3000,
): Promise<T> {
  return retry(fn, (result) => result !== null, maxAttempts, delayMs) as Promise<T>;
}

// Minimal query used only to resolve a block hash — omits new schema fields so it
// works against older indexer deployments that would return data:null for the full fragment.
const RESOLVE_BLOCK_HASH_QUERY = `
query GetTransactionByOffset($OFFSET: TransactionOffset!) {
  transactions(offset: $OFFSET) {
    hash
    block {
      hash
    }
  }
}`;

/**
 * When the toolkit doesn't provide a block hash (newer output format),
 * resolve it from the indexer by looking up the transaction.
 */
export async function resolveBlockHash(result: ToolkitTransactionResult): Promise<void> {
  if (result.blockHash || !result.txHash) return;
  log.debug(
    `Block hash missing from toolkit output, resolving from indexer for tx ${result.txHash}`,
  );
  const client = new IndexerHttpClient();
  const offset = { hash: result.txHash };
  const txResponse = await retry(
    () => client.getTransactionByOffset(offset, RESOLVE_BLOCK_HASH_QUERY),
    (response) => response.data?.transactions != null && response.data.transactions.length > 0,
    30,
    2000,
  );
  const transactions = txResponse?.data?.transactions ?? [];
  // Callers of resolveBlockHash invoke it only on success-path txs that haven't
  // been retried, so we don't expect multiple records per hash. If a caller adds
  // a path that can produce multiple records (e.g. a failed retry sharing the
  // hash), restore the SUCCESS-preferring filter or pick a different signal.
  const tx = transactions[0];
  if (tx?.block?.hash) {
    result.blockHash = tx.block.hash;
    log.debug(`Resolved block hash: ${result.blockHash}`);
  } else {
    log.warn(`Could not resolve block hash from indexer for tx ${result.txHash}`);
  }
}

export function getBlockByHashWithRetry(hash: string): Promise<BlockResponse> {
  return retry(
    () => new IndexerHttpClient().getBlockByOffset({ hash: hash }),
    (response) => response.data?.block != null,
    30,
    2000,
  );
}

export function getTransactionByHashWithRetry(hash: string): Promise<TransactionResponse> {
  return retry(
    () => new IndexerHttpClient().getTransactionByOffset({ hash: hash }),
    (response) => response.data?.transactions != null && response.data.transactions.length > 0,
    30,
    2000,
  );
}

export function getContractDeploymentHashes(
  contractAddress: string,
): Promise<{ txHash: string; blockHash: string }> {
  return retry(
    async () => {
      const indexerClient = new IndexerHttpClient();
      const contractActionResponse = await indexerClient.getContractAction(contractAddress);

      if (contractActionResponse?.data?.contractAction?.__typename === 'ContractDeploy') {
        const contractAction = contractActionResponse.data.contractAction;
        const txHash = contractAction.transaction?.hash || '';
        const blockHash = contractAction.transaction?.block?.hash || '';

        if (!txHash || !blockHash) {
          throw new Error('Missing transaction hash or block hash in contract deployment');
        }

        return { txHash, blockHash };
      }

      throw new Error('Contract action is not a deployment or not found');
    },
    (result) => result.txHash !== '' && result.blockHash !== '',
    30,
    2000,
  );
}

/**
 * Wait until the provided events array stabilizes (its length stops changing between checks),
 * then drain and return its contents.
 *
 * Checks every `intervalMs` milliseconds and bails out after `maxWaitMs` to avoid infinite waits.
 */
export async function waitForEventsStabilization<T>(
  events: T[],
  intervalMs: number = 500,
  maxWaitMs: number = 60000,
): Promise<T[]> {
  let previousCount = -1;
  const start = Date.now();

  while (true) {
    await new Promise((resolve) => setTimeout(resolve, intervalMs));
    const currentCount = events.length;
    if (currentCount === previousCount) {
      return events.splice(0, events.length);
    }
    previousCount = currentCount;
    if (Date.now() - start > maxWaitMs) {
      return events.splice(0, events.length);
    }
  }
}

/**
 * Helper function to validate data against a Zod schema.
 * This is a common pattern used across e2e tests for schema validation.
 *
 * @param data - The data to validate
 * @param schema - The Zod schema to validate against
 * @param dataType - A descriptive name for the data type (used in error messages)
 * @throws Error if validation fails
 */
export function validateSchema<T>(data: T, schema: z.ZodSchema<T>, dataType: string): void {
  log.debug(`Validating ${dataType} schema`);
  const validationResult = schema.safeParse(data);
  if (!validationResult.success) {
    throw new Error(
      `${dataType} schema validation failed: ${JSON.stringify(validationResult.error, null, 2)}`,
    );
  }
}

/**
 *  Prepares wallet event subscriptions for unshielded-transaction tests.
 *
 * Subscribes the source and a number of destination wallets to unshielded transaction events,
 *  waits for initial event streams to stabilise and returns all relevant context needed before performing any transactions.
 */
export async function setupWalletEventSubscriptions(
  toolkit: ToolkitWrapper,
  indexerWsClient: IndexerWsClient,
  sourceSeed: string,
  destinationSeeds: string[],
) {
  // Getting the addresses from their seeds
  const sourceAddress = (await toolkit.showAddress(sourceSeed)).unshielded;
  // Events from the indexer websocket for both the source addresses
  const sourceAddressEvents: UnshieldedTxSubscriptionResponse[] = [];

  // Subscribe the source wallet to unshielded transaction events
  const sourceAddrUnscribeFromEvents = indexerWsClient.subscribeToUnshieldedTransactionEvents(
    { next: (event) => sourceAddressEvents.push(event) },
    { address: sourceAddress },
  );

  // Historical events from the indexer websocket for both the source addresses:
  // wait until source events count stabilizes, then snapshot to historical array
  const historicalSourceEvents = await waitForEventsStabilization(sourceAddressEvents, 1000);

  // Derive and subscribe ALL destination wallets dynamically
  const destinationWallets = await Promise.all(
    destinationSeeds.map(async (seed) => {
      const destinationAddress = (await toolkit.showAddress(seed)).unshielded;

      const events: UnshieldedTxSubscriptionResponse[] = [];

      // Subscribe the destination wallet to unshielded transaction events
      const unsubscribe = indexerWsClient.subscribeToUnshieldedTransactionEvents(
        { next: (event) => events.push(event) },
        { address: destinationAddress },
      );
      // Historical events captured before submitting the transaction: wait until
      // destination events count stabilizes, then snapshot to historical array
      const historicalDestinationEvents = await waitForEventsStabilization(events, 1000);

      return {
        seed,
        destinationAddress,
        events,
        unsubscribe,
        historicalDestinationEvents,
      };
    }),
  );
  return {
    source: {
      seed: sourceSeed,
      address: sourceAddress,
      events: sourceAddressEvents,
      unsubscribe: sourceAddrUnscribeFromEvents,
      historicalEvents: historicalSourceEvents,
    },
    destinations: destinationWallets,
  };
}

/**
 * Extracts all unshielded transaction events of a specific GraphQL `__typename`
 * from a wallet’s subscription event stream.
 */
export function getEventsOfType<T extends string>(
  events: UnshieldedTxSubscriptionResponse[],
  type: T,
) {
  return events
    .map((e) => e.data?.unshieldedTransactions)
    .filter((tx): tx is Extract<typeof tx, { __typename: T }> => tx?.__typename === type);
}
