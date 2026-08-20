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

// The unshielded transfer scenario, shared by the two unshielded e2e suites:
// night-transactions.test.ts (the native NIGHT token) and
// custom-unshielded-token-transactions.test.ts (a contract-minted custom token).
// Both suites exercise the same transfer and the same indexer surfaces; the token
// type is the only parameter, so the coverage cannot silently drift apart between
// them.
//
// Not a *.test.ts file, so it is never collected on its own — it only defines
// tests when a suite calls defineUnshieldedTransferTests.

import type { TestContext } from 'vitest';
import log from '@utils/logging/logger';
import '@utils/logging/test-logging-hooks';
import { retry } from '@utils/retry-helper';
import dataProvider from '@utils/testdata-provider';
import { IndexerHttpClient } from '@utils/indexer/http-client';
import { IndexerWsClient, UnshieldedTxSubscriptionResponse } from '@utils/indexer/websocket-client';
import { ToolkitWrapper, ToolkitTransactionResult } from '@utils/toolkit/toolkit-wrapper';
import {
  Transaction,
  UnshieldedTransaction,
  UnshieldedTransactionEvent,
  UnshieldedTransactionsProgress,
  UnshieldedUtxo,
} from '@utils/indexer/indexer-types';
import {
  getBlockByHashWithRetry,
  getEventsOfType,
  resolveBlockHash,
  setupWalletEventSubscriptions,
} from './test-utils';

/** Timeout for the suite and for the transfer that sets it up. */
export const UNSHIELDED_TRANSFER_TIMEOUT = 200_000;

/** NIGHT is the chain's native unshielded token: its token type is all zeros. */
export const NIGHT_TOKEN_TYPE = '0'.repeat(64);

/**
 * The progress subscription backs off while idle (since indexer 4.4.0: 30s doubling
 * up to 240s, ±20% jitter), so a subscription opened well before the transaction may
 * only report the change after the full backed-off gap — about five minutes worst
 * case. Hence the wide window on the two progress tests.
 */
const PROGRESS_TIMEOUT = 400_000;

/** Identifies a shared test, so a suite can attach its Xray key to the right one. */
export type UnshieldedTransferTestId =
  | 'blockQueryByHash'
  | 'transactionQueryByHash'
  | 'sourceTransactionEvent'
  | 'destinationTransactionEvent'
  | 'transferredAmount'
  | 'tokenTypeOnOutputs'
  | 'sourceProgressUpdate'
  | 'destinationProgressUpdate';

/** The unshielded token a suite exercises the scenario with. */
export interface UnshieldedTokenUnderTest {
  /** Short name used in test titles and in each test's `labels`, e.g. `NIGHT`. */
  label: string;
  /**
   * Hex token type to transfer, and the type every UTXO of the transfer must carry.
   * A suite that picks its token at runtime may replace it from `prepare`, which runs
   * before the transfer is submitted.
   */
  tokenType: string;
  /** Amount to transfer, with the unit name used in test titles (e.g. `1` / `STAR`). */
  amount: number;
  unit: string;
  /**
   * Seed of the receiving wallet, which must be distinct from every other e2e suite's
   * destination. The e2e files run concurrently against one chain, so a shared
   * destination lets one suite's transfer show up in the other's event stream — and
   * where a suite asserts that a wallet received no transaction event at all (the
   * NIGHT suite's multi-destination tests do), that turns into a false failure.
   */
  destinationSeed: string;
  /**
   * Seeds of further wallets to subscribe alongside the destination, exposed as
   * `wallet.destinations[1..]`. A suite adds them when it also asserts on how the
   * indexer routes a transfer between several subscribed recipients.
   */
  extraDestinationSeeds?: string[];
  /** Xray test keys by test id. Omitted by suites with no Xray coverage. */
  testKeys?: Partial<Record<UnshieldedTransferTestId, string>>;
  /**
   * Runs once the wallets are subscribed and before the transfer is submitted —
   * the place to capture pre-transfer baselines, and the place to decide whether
   * the environment can run the scenario at all.
   *
   * @returns a reason to skip every test, or null when the scenario can run.
   */
  prepare?: (scenario: UnshieldedTransferScenario) => Promise<string | null>;
}

/** Shared state of one transfer, populated by the scenario's `beforeAll`. */
export interface UnshieldedTransferScenario {
  token: UnshieldedTokenUnderTest;
  httpClient: IndexerHttpClient;
  wsClient: IndexerWsClient;
  toolkit: ToolkitWrapper;
  wallet: Awaited<ReturnType<typeof setupWalletEventSubscriptions>>;
  transactionResult: ToolkitTransactionResult;
  /** Non-null when the environment cannot run the scenario; every test skips with it. */
  skipReason: string | null;
}

/**
 * Registers the scenario's `beforeAll`/`afterAll` hooks: connect, start the toolkit,
 * subscribe both wallets, then transfer `amount` of the token under test from the
 * environment's funding wallet to the destination wallet.
 *
 * @param token - The token to exercise the scenario with.
 * @returns The scenario state, readable from test bodies once `beforeAll` has run.
 */
export function setupUnshieldedTransferScenario(
  token: UnshieldedTokenUnderTest,
): UnshieldedTransferScenario {
  // Every other field is assigned by the beforeAll below, before any test body reads it.
  const scenario = { token, skipReason: null } as UnshieldedTransferScenario;

  beforeAll(async () => {
    scenario.httpClient = new IndexerHttpClient();
    scenario.wsClient = new IndexerWsClient();
    await scenario.wsClient.connectionInit();

    scenario.toolkit = new ToolkitWrapper({});
    await scenario.toolkit.start();

    const sourceSeed = dataProvider.getFundingSeed();
    scenario.wallet = await setupWalletEventSubscriptions(
      scenario.toolkit,
      scenario.wsClient,
      sourceSeed,
      [token.destinationSeed, ...(token.extraDestinationSeeds ?? [])],
    );

    scenario.skipReason = (await token.prepare?.(scenario)) ?? null;
    if (scenario.skipReason !== null) {
      log.warn(`Skipping the ${token.label} transfer scenario: ${scenario.skipReason}`);
      return;
    }

    scenario.transactionResult = await scenario.toolkit.generateSingleTx(
      sourceSeed,
      'unshielded',
      scenario.wallet.destinations[0].destinationAddress,
      token.amount,
      token.tokenType,
    );
    await resolveBlockHash(scenario.transactionResult);
  }, UNSHIELDED_TRANSFER_TIMEOUT);

  afterAll(async () => {
    scenario.wallet?.source.unsubscribe();
    scenario.wallet?.destinations.forEach((destination) => destination.unsubscribe());
    await Promise.all([scenario.toolkit?.stop(), scenario.wsClient?.connectionClose()]);
  });

  return scenario;
}

/**
 * Starts a shared test: attaches its labels and its suite's Xray key, then skips it
 * when the environment could not be prepared.
 */
function startTest(
  scenario: UnshieldedTransferScenario,
  ctx: TestContext,
  id: UnshieldedTransferTestId,
  labels: string[],
): void {
  const testKey = scenario.token.testKeys?.[id];
  ctx.task!.meta.custom = {
    labels: [...labels, 'UnshieldedTokens', scenario.token.label],
    ...(testKey ? { testKey } : {}),
  };
  ctx.skip?.(scenario.skipReason !== null, scenario.skipReason ?? '');
}

/** Skips a test whose subject is the confirmed transaction itself. */
function skipUnlessConfirmed(scenario: UnshieldedTransferScenario, ctx: TestContext): void {
  ctx.skip?.(
    scenario.transactionResult.status !== 'confirmed',
    "Toolkit transaction hasn't been confirmed",
  );
}

/** Returns the UTXOs that carry the token type under test. */
function ofTokenUnderTest(
  scenario: UnshieldedTransferScenario,
  utxos: UnshieldedUtxo[] | undefined,
): UnshieldedUtxo[] {
  return (utxos ?? []).filter(
    (utxo: UnshieldedUtxo) => utxo.tokenType === scenario.token.tokenType,
  );
}

/** Sums UTXO values. On-chain values are u128, so they are summed as BigInt. */
function totalValue(utxos: UnshieldedUtxo[]): bigint {
  return utxos.reduce((total, utxo) => total + BigInt(utxo.value), 0n);
}

/**
 * Fetches the transfer under test from its block, selected by the transaction hash the
 * toolkit reported. Selecting by hash rather than by "the first transaction with
 * unshielded activity" is what keeps the assertions on this transfer: the sibling suite
 * transfers from the same funding wallet, and a busy chain carries other traffic.
 */
async function fetchTransactionUnderTest(
  scenario: UnshieldedTransferScenario,
): Promise<Transaction> {
  const blockResponse = await getBlockByHashWithRetry(scenario.transactionResult.blockHash);
  const transaction = blockResponse.data?.block?.transactions?.find(
    (tx: Transaction) => tx.hash === scenario.transactionResult.txHash,
  );
  expect(
    transaction,
    `Transaction ${scenario.transactionResult.txHash} is not in block ${scenario.transactionResult.blockHash}`,
  ).toBeDefined();
  return transaction!;
}

/**
 * Waits for a wallet's subscription to deliver the transaction event of the transfer
 * under test, matched by the transaction hash the toolkit reported — the source address
 * is the shared funding wallet, so matching on the owner alone would also be satisfied
 * by the sibling suite's concurrent transfer.
 */
async function findTransferEvent(
  scenario: UnshieldedTransferScenario,
  events: UnshieldedTxSubscriptionResponse[],
  addressLabel: string,
): Promise<UnshieldedTransaction> {
  return retry(
    async () => {
      const event = getEventsOfType(events, 'UnshieldedTransaction').find(
        (txEvent) => txEvent.transaction.hash === scenario.transactionResult.txHash,
      );
      if (!event) {
        throw new Error(`${addressLabel} address transaction event not found yet`);
      }
      return event;
    },
    {
      maxRetries: 10,
      delayMs: 3000,
      retryLabel: `find ${addressLabel} address transaction event`,
    },
  );
}

/**
 * Finds a progress update event reporting a transaction id past the baseline.
 * Used by the source and destination progress tests through `retry`.
 *
 * @param events - The events array to search.
 * @param baselineTransactionId - The transaction ID to compare against.
 * @param addressLabel - Label for error messages ('source' or 'destination').
 * @returns The found event.
 * @throws Error if no matching event is found.
 */
function findProgressUpdateEvent(
  events: UnshieldedTxSubscriptionResponse[],
  baselineTransactionId: number,
  addressLabel: string,
): UnshieldedTxSubscriptionResponse {
  const event = events.find((event) => {
    const txEvent = event.data?.unshieldedTransactions as UnshieldedTransactionEvent;

    log.debug(`waiting for UnshieldedTransactionsProgress event`);
    if (txEvent.__typename === 'UnshieldedTransactionsProgress') {
      const progressUpdate = txEvent;
      log.debug(`progressUpdate received: ${JSON.stringify(progressUpdate, null, 2)}`);
      if (progressUpdate.highestTransactionId > baselineTransactionId) {
        return true;
      }
    }
  });
  if (!event) {
    throw new Error(`${addressLabel} address progress update event not found yet`);
  }
  return event;
}

/** Asserts a progress update past the baseline arrives for one of the two addresses. */
async function expectProgressUpdate(
  events: UnshieldedTxSubscriptionResponse[],
  historicalEvents: UnshieldedTxSubscriptionResponse[],
  addressLabel: string,
): Promise<void> {
  const isProgress = (event: UnshieldedTxSubscriptionResponse) =>
    event.data?.unshieldedTransactions.__typename === 'UnshieldedTransactionsProgress';

  // The indexer sends a progress update as soon as a wallet subscribes, so the snapshot
  // taken before the transfer must hold one — including for a wallet that has never
  // transacted, which gets one reporting 0. Without that check a snapshot that merely
  // arrived too late would silently baseline at 0, and against an environment where the
  // wallet has transacted before, the first replayed event would clear a `> 0` bar
  // without the transfer under test having been observed at all.
  const historicalProgress = historicalEvents.filter(isProgress);
  expect(
    historicalProgress.length,
    `No ${addressLabel} progress update was captured before the transfer, so there is no baseline to compare against`,
  ).toBeGreaterThan(0);

  const highestTransactionIdBefore =
    (
      historicalProgress.at(-1)?.data?.unshieldedTransactions as
        UnshieldedTransactionsProgress | undefined
    )?.highestTransactionId ?? 0;
  log.info(
    `Highest ${addressLabel} transaction ID before transaction: ${highestTransactionIdBefore}`,
  );

  const event = await retry(
    async () => findProgressUpdateEvent(events, highestTransactionIdBefore, addressLabel),
    {
      maxRetries: 60,
      delayMs: 5000,
      retryLabel: `find ${addressLabel} address progress update event`,
    },
  );

  expect(event).toBeDefined();
  const highestTransactionIdAfter = (
    event.data?.unshieldedTransactions as UnshieldedTransactionsProgress
  ).highestTransactionId;
  log.info(
    `Highest ${addressLabel} transaction ID after transaction: ${highestTransactionIdAfter}`,
  );
  expect(highestTransactionIdAfter).toBeGreaterThan(highestTransactionIdBefore);
}

/**
 * Defines the tests every unshielded token type must pass: the transfer is reported
 * by block query, transaction query, both wallets' transaction subscriptions and both
 * wallets' progress updates, with the right amount and the right token type.
 *
 * @param scenario - The scenario returned by `setupUnshieldedTransferScenario`.
 */
export function defineUnshieldedTransferTests(scenario: UnshieldedTransferScenario): void {
  const { amount, unit } = scenario.token;

  describe(`a successful unshielded transaction transferring ${amount} ${unit} between two addresses`, () => {
    /**
     * Once an unshielded transaction has been submitted to node and confirmed, the indexer should
     * report that transaction in the block through a block query by hash, using the block hash
     * reported by the toolkit.
     *
     * @given a confirmed unshielded transaction between two wallets
     * @when the block is queried by the block hash the toolkit reported
     * @then the block should contain the transaction with outputs for both addresses
     */
    test('should be reported by the indexer through a block query by hash', async (ctx: TestContext) => {
      startTest(scenario, ctx, 'blockQueryByHash', ['Query', 'Block', 'ByHash']);
      skipUnlessConfirmed(scenario, ctx);

      // The expected block might take a bit more to show up by indexer, so we retry a few times
      const blockResponse = await getBlockByHashWithRetry(scenario.transactionResult.blockHash);

      expect(blockResponse?.data?.block?.transactions).toBeDefined();
      expect(blockResponse?.data?.block?.transactions?.length).toBeGreaterThan(0);

      const sourceAddresInTx = blockResponse.data?.block?.transactions?.find((tx: Transaction) =>
        tx.unshieldedCreatedOutputs?.find(
          (output: UnshieldedUtxo) => output.owner === scenario.wallet.source.address,
        ),
      );

      const destAddresInTx = blockResponse.data?.block?.transactions?.find((tx: Transaction) =>
        tx.unshieldedCreatedOutputs?.find(
          (output: UnshieldedUtxo) =>
            output.owner === scenario.wallet.destinations[0].destinationAddress,
        ),
      );

      expect(sourceAddresInTx).toBeDefined();
      expect(destAddresInTx).toBeDefined();
    });

    /**
     * Once an unshielded transaction has been submitted to node and confirmed, the indexer should
     * report that transaction through a query by transaction hash, using the transaction hash
     * reported by the toolkit.
     *
     * @given a confirmed unshielded transaction between two wallets
     * @when transactions are queried by the transaction hash
     * @then the returned transactions should include outputs for both addresses involved
     */
    test('should be reported by the indexer through a transaction query by hash', async (ctx: TestContext) => {
      startTest(scenario, ctx, 'transactionQueryByHash', ['Query', 'Transaction', 'ByHash']);
      skipUnlessConfirmed(scenario, ctx);

      // The expected transaction might take a bit more to show up by indexer, so we retry a few times
      const transactionResponse = await scenario.httpClient.getTransactionByOffset({
        hash: scenario.transactionResult.txHash,
      });

      expect(transactionResponse?.data?.transactions).toBeDefined();
      expect(
        transactionResponse?.data?.transactions?.length,
        'No transactions found',
      ).toBeGreaterThan(0);

      const sourceAddresInTx = transactionResponse.data?.transactions?.find((tx: Transaction) =>
        tx.unshieldedCreatedOutputs?.find(
          (output: UnshieldedUtxo) => output.owner === scenario.wallet.source.address,
        ),
      );
      expect(sourceAddresInTx).toBeDefined();

      const destAddresInTx = transactionResponse.data?.transactions?.find((tx: Transaction) =>
        tx.unshieldedCreatedOutputs?.find(
          (output: UnshieldedUtxo) =>
            output.owner === scenario.wallet.destinations[0].destinationAddress,
        ),
      );
      expect(destAddresInTx).toBeDefined();
    });

    /**
     * Once an unshielded transaction has been submitted to node and confirmed, the indexer should
     * report that transaction through an unshielded transaction event for the source address.
     *
     * @given a subscription to unshielded transaction events for the source address
     * @when an unshielded transaction is submitted to node
     * @then the event of that very transaction hash is received, and the UTXOs it spends
     *       carry the token type under test and belong to the source address
     */
    test('should be reported by the indexer through an unshielded transaction event for the source address', async (ctx: TestContext) => {
      startTest(scenario, ctx, 'sourceTransactionEvent', ['Subscription', 'Transaction']);
      skipUnlessConfirmed(scenario, ctx);

      const sourceAddressEvent = await findTransferEvent(
        scenario,
        scenario.wallet.source.events,
        'source',
      );

      const spentUtxos = ofTokenUnderTest(scenario, sourceAddressEvent.spentUtxos);
      expect(spentUtxos.length).toBeGreaterThan(0);
      expect(spentUtxos.every((utxo) => utxo.owner === scenario.wallet.source.address)).toBe(true);
      expect(ofTokenUnderTest(scenario, sourceAddressEvent.createdUtxos).length).toBeGreaterThan(0);
    });

    /**
     * Once an unshielded transaction has been submitted to node and confirmed, the indexer should
     * report that transaction through an unshielded transaction event for the destination address.
     *
     * @given a subscription to unshielded transaction events for the destination address
     * @when an unshielded transaction is submitted to node
     * @then the event of that very transaction hash is received, holding a single created
     *       UTXO for the destination of the amount sent, in the token type under test
     */
    test('should be reported by the indexer through an unshielded transaction event for the destination address', async (ctx: TestContext) => {
      startTest(scenario, ctx, 'destinationTransactionEvent', ['Subscription', 'Transaction']);
      skipUnlessConfirmed(scenario, ctx);

      const destinationAddressEvent = await findTransferEvent(
        scenario,
        scenario.wallet.destinations[0].events,
        'destination',
      );

      const receivedUtxos = ofTokenUnderTest(scenario, destinationAddressEvent.createdUtxos).filter(
        (utxo) => utxo.owner === scenario.wallet.destinations[0].destinationAddress,
      );
      expect(receivedUtxos.map((utxo) => utxo.value)).toEqual([String(amount)]);
    });

    /**
     * A transfer pays the destination the amount sent and returns the rest of what it
     * spent to the source as change, so the created and spent outputs of the token type
     * under test have to account for exactly that. How many inputs coin selection takes
     * is its own business, so nothing here is asserted on their number.
     *
     * @given a confirmed unshielded transaction between two wallets
     * @when that transaction is looked up in its block and its outputs of the token
     *       under test are inspected
     * @then the destination holds a single created output of the amount sent, every
     *       spent output belongs to the source, and the source's change accounts for
     *       the rest of what was spent
     */
    test(`should have transferred ${amount} ${unit} from the source to the destination address`, async (ctx: TestContext) => {
      startTest(scenario, ctx, 'transferredAmount', []);
      skipUnlessConfirmed(scenario, ctx);

      const unshieldedTx = await fetchTransactionUnderTest(scenario);
      const createdOutputs = ofTokenUnderTest(scenario, unshieldedTx.unshieldedCreatedOutputs);
      const spentOutputs = ofTokenUnderTest(scenario, unshieldedTx.unshieldedSpentOutputs);
      log.info(
        `Transaction ${unshieldedTx.hash} moved ${amount} ${unit} through ` +
          `${spentOutputs.length} spent and ${createdOutputs.length} created output(s)`,
      );

      const destinationOutputs = createdOutputs.filter(
        (output) => output.owner === scenario.wallet.destinations[0].destinationAddress,
      );
      expect(destinationOutputs.map((output) => output.value)).toEqual([String(amount)]);

      expect(spentOutputs.length).toBeGreaterThan(0);
      expect(spentOutputs.every((output) => output.owner === scenario.wallet.source.address)).toBe(
        true,
      );

      const changeOutputs = createdOutputs.filter(
        (output) => output.owner === scenario.wallet.source.address,
      );
      expect(totalValue(changeOutputs)).toBe(totalValue(spentOutputs) - BigInt(amount));
    });

    /**
     * A transfer moves one unshielded token, so the UTXOs it creates and spends must carry
     * the token type the transfer was made in — the type the suite asked the toolkit for,
     * never a value read back from the response under test.
     *
     * @given a confirmed unshielded transaction between two wallets
     * @when the created and spent outputs of that transaction are inspected
     * @then the token type under test is among them, and the only other one tolerated is
     *       NIGHT, which a transfer of any other token may touch to pay for itself
     */
    test('should report the token type under test on every created and spent output', async (ctx: TestContext) => {
      startTest(scenario, ctx, 'tokenTypeOnOutputs', ['Query', 'Block', 'ByHash']);
      skipUnlessConfirmed(scenario, ctx);

      const unshieldedTx = await fetchTransactionUnderTest(scenario);
      const tokenTypes = [
        ...new Set(
          [
            ...(unshieldedTx.unshieldedCreatedOutputs ?? []),
            ...(unshieldedTx.unshieldedSpentOutputs ?? []),
          ].map((utxo: UnshieldedUtxo) => utxo.tokenType),
        ),
      ];

      expect(tokenTypes).toContain(scenario.token.tokenType);
      const unexpected = tokenTypes.filter(
        (tokenType) => tokenType !== scenario.token.tokenType && tokenType !== NIGHT_TOKEN_TYPE,
      );
      expect(unexpected).toEqual([]);
    });

    /**
     * Once an unshielded transaction has been submitted to node and confirmed, the indexer should
     * report that transaction through a progress update event for the source address.
     *
     * @given a subscription to unshielded transaction events for the source address
     * @when an unshielded transaction is submitted to node
     * @then a progress update event is received
     * @and its highest transaction ID is greater than the one seen before the transaction
     */
    test(
      'should be reported by the indexer through a progress update event for the source address',
      { timeout: PROGRESS_TIMEOUT },
      async (ctx: TestContext) => {
        startTest(scenario, ctx, 'sourceProgressUpdate', [
          'Subscription',
          'Transaction',
          'Progress',
        ]);

        await expectProgressUpdate(
          scenario.wallet.source.events,
          scenario.wallet.source.historicalEvents,
          'source',
        );
      },
    );

    /**
     * Once an unshielded transaction has been submitted to node and confirmed, the indexer should
     * report that transaction through a progress update event for the destination address.
     *
     * @given a subscription to unshielded transaction events for the destination address
     * @when an unshielded transaction is submitted to node
     * @then a progress update event is received
     * @and its highest transaction ID is greater than the one seen before the transaction
     */
    test(
      'should be reported by the indexer through a progress update event for the destination address',
      { timeout: PROGRESS_TIMEOUT },
      async (ctx: TestContext) => {
        startTest(scenario, ctx, 'destinationProgressUpdate', [
          'Subscription',
          'Transaction',
          'Progress',
        ]);

        await expectProgressUpdate(
          scenario.wallet.destinations[0].events,
          scenario.wallet.destinations[0].historicalDestinationEvents,
          'destination',
        );
      },
    );
  });
}
