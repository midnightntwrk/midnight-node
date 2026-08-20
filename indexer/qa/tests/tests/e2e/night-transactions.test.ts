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

import type { TestContext } from 'vitest';
import log from '@utils/logging/logger';
import '@utils/logging/test-logging-hooks';
import { EventCoordinator } from '@utils/event-coordinator';
import { DustLedgerEventsUnionSchema } from '@utils/indexer/graphql/schema';
import {
  isUnshieldedTransaction,
  RegularTransaction,
  Transaction,
  UnshieldedTransactionEvent,
  UnshieldedUtxo,
} from '@utils/indexer/indexer-types';
import { IndexerWsClient, UnshieldedTxSubscriptionResponse } from '@utils/indexer/websocket-client';
import { collectValidDustLedgerEvents } from 'tests/shared/dust-ledger-utils';
import { getEventsOfType, retrySimple, waitForEventsStabilization } from './test-utils';
import {
  defineUnshieldedTransferTests,
  NIGHT_TOKEN_TYPE,
  setupUnshieldedTransferScenario,
  UNSHIELDED_TRANSFER_TIMEOUT,
} from './unshielded-transfer-scenario';

// Destination wallets are numbered per e2e suite (…e2e002 is the custom-token suite's) so
// that no two suites share one — see `destinationSeed` in unshielded-transfer-scenario.ts.
// The seed this suite used before, …987654321, is shielded-transactions.test.ts's
// destination.
const DESTINATION_SEED = '0000000000000000000000000000000000000000000000000000000000e2e001';

// A second destination for this suite, subscribed on the same WS connection, so the
// multi-destination tests below can assert the indexer routes each transfer to the intended
// recipient only. It must stay unused elsewhere: those tests assert it receives no
// transaction event at all.
const SECOND_DESTINATION_SEED = '000000000000000000000000000000000000000000000000000000000e2e001b';

/**
 * Validates that an unshielded transaction is reported consistently across the event streams of
 * the source and the destination wallet.
 *
 * Each destination transaction is paired with the source transaction carrying the same hash, then
 * deep-checked for identical transaction identity, UTXO ownership and output indices, creation
 * time, dust registration flags, spent-UTXO cross-links and value conservation.
 *
 * @param srcTxs - Events emitted for the **source** wallet.
 * @param destTxs - Events emitted for the **destination** wallet.
 * @param srcAddr - Source wallet address, expected to own the change UTXO at outputIndex 1.
 * @param destAddr - Destination wallet address, expected to own the created UTXO at outputIndex 0.
 * @param expectedValue - Value the destination is expected to receive.
 *
 * - Uses `isUnshieldedTransaction()` to filter out `UnshieldedTransactionsProgress` events.
 */
function validateCrossWalletTransaction(
  srcTxs: UnshieldedTransactionEvent[],
  destTxs: UnshieldedTransactionEvent[],
  srcAddr: string,
  destAddr: string,
  expectedValue: string,
) {
  const validSrcTxs = srcTxs.filter(isUnshieldedTransaction);
  const validDestTxs = destTxs.filter(isUnshieldedTransaction);

  log.debug(validSrcTxs, `Source transactions for ${srcAddr}`);
  log.debug(validDestTxs, `Destination transactions for ${destAddr}`);

  if (!validDestTxs.length) {
    throw new Error(`No UnshieldedTransaction events for ${destAddr} — expected at least one.`);
  }

  validDestTxs.forEach((destTx) => {
    const srcTx = validSrcTxs.find((s) => s.transaction.hash === destTx.transaction.hash);
    if (!srcTx) {
      throw new Error(`No matching source transaction found for hash ${destTx.transaction.hash}`);
    }

    const srcUtxo = srcTx.createdUtxos[0];
    const destUtxo = destTx.createdUtxos[0];

    // Value & identity
    expect(destUtxo.value).toBe(expectedValue);
    expect(BigInt(srcUtxo.value)).toBeGreaterThan(BigInt(destUtxo.value));
    expect(destTx.transaction.hash).toBe(srcTx.transaction.hash);
    expect(destTx.transaction.id).toBe(srcTx.transaction.id);

    // Ownership & indices
    expect(srcUtxo.owner).toBe(srcAddr);
    expect(destUtxo.owner).toBe(destAddr);
    expect(destUtxo.outputIndex).toBe(0);
    expect(srcUtxo.outputIndex).toBe(1);

    // Creation time alignment
    expect(srcUtxo.ctime).toBe(destUtxo.ctime);

    // Dust registration flags. This asymmetry holds only because the source is the funding wallet,
    // which is registered for dust generation, while the destinations are fresh, unregistered ones.
    expect(destUtxo.registeredForDustGeneration).toBe(false);
    expect(srcUtxo.registeredForDustGeneration).toBe(true);

    // Cross-link consistency
    expect(srcUtxo.createdAtTransaction.hash).toBe(destTx.transaction.hash);

    // The source funds the transfer by spending an input, so its stream must carry a spent UTXO.
    // Assert that rather than guarding on it, or the checks below silently disappear.
    expect(
      srcTx.spentUtxos,
      `No spent UTXO in the source stream for ${srcAddr} on hash ${destTx.transaction.hash}`,
    ).not.toHaveLength(0);

    const spent = srcTx.spentUtxos[0];
    expect(spent.spentAtTransaction.hash).toBe(destTx.transaction.hash);
    const spentTx = spent.spentAtTransaction as { hash: string; identifiers?: string[] };
    expect(spentTx.identifiers?.[0]).toBe(destTx.transaction.identifiers?.[0]);

    // Value conservation: the destination receives the spent input minus the change kept by source.
    expect(BigInt(destUtxo.value)).toBe(BigInt(spent.value) - BigInt(srcUtxo.value));

    log.debug(`Validation complete for hash=${destTx.transaction.hash}`);
  });
}

describe('unshielded NIGHT transactions', { timeout: UNSHIELDED_TRANSFER_TIMEOUT }, () => {
  const indexerEventCoordinator = new EventCoordinator();

  // Distinct unshielded token types held by the genesis block. A dev chain is minted
  // with NIGHT alone, so this is what the genesis identity test checks NIGHT against.
  let genesisTokenTypes: string[];
  let previousMaxDustId: number;
  let dustCommitmentEndIndexBeforeTx: number;

  const scenario = setupUnshieldedTransferScenario({
    label: 'NIGHT',
    tokenType: NIGHT_TOKEN_TYPE,
    amount: 1,
    unit: 'STAR',
    destinationSeed: DESTINATION_SEED,
    extraDestinationSeeds: [SECOND_DESTINATION_SEED],
    testKeys: {
      blockQueryByHash: 'PM-17711',
      transactionQueryByHash: 'PM-17712',
      sourceTransactionEvent: 'PM-17713',
      destinationTransactionEvent: 'PM-17714',
      transferredAmount: 'PM-17715',
    },
    prepare: async (scenario) => {
      const beforeEvents = await collectValidDustLedgerEvents(
        scenario.wsClient,
        indexerEventCoordinator,
        1,
      );
      previousMaxDustId = beforeEvents[0].data!.dustLedgerEvents.maxId;
      log.debug(`Previous max dust ID before tx = ${previousMaxDustId}`);

      // Capture the highest dustCommitmentEndIndex before the transaction from the genesis
      // block. Guard against null data: older indexer deployments return a GraphQL validation
      // error when the query includes schema fields not yet in that version, which sets data
      // to null.
      const genesisResponse = await scenario.httpClient.getBlockByOffset({ height: 0 });
      const genesisTxs = genesisResponse.data?.block?.transactions ?? [];
      dustCommitmentEndIndexBeforeTx = genesisTxs.reduce((max, tx) => {
        const regularTx = tx as RegularTransaction;
        return regularTx.dustCommitmentEndIndex != null && regularTx.dustCommitmentEndIndex > max
          ? regularTx.dustCommitmentEndIndex
          : max;
      }, 0);
      log.debug(`Highest dustCommitmentEndIndex from genesis = ${dustCommitmentEndIndexBeforeTx}`);

      genesisTokenTypes = [
        ...new Set(
          genesisTxs
            .flatMap((tx) => (tx as RegularTransaction).unshieldedCreatedOutputs ?? [])
            .map((utxo: UnshieldedUtxo) => utxo.tokenType),
        ),
      ];
      log.debug(`Unshielded token types in genesis = ${genesisTokenTypes.join(', ')}`);

      // NIGHT is on the chain from genesis, so the scenario is always runnable.
      return null;
    },
  });

  defineUnshieldedTransferTests(scenario);

  describe('the genesis block of a chain minted with NIGHT alone', () => {
    /**
     * NIGHT's token type is the all-zeros type, and a dev chain's genesis block is minted
     * with NIGHT alone. Asserting that identity here is what keeps the transfer tests
     * honest: they compare against the same constant instead of against a token type read
     * back from the very response they are checking, so a systemic mis-decode cannot pass.
     *
     * @given a chain whose genesis block was minted with NIGHT alone
     * @when the unshielded outputs created in the genesis block are inspected
     * @then they all carry the all-zeros NIGHT token type (0x00…00)
     */
    test('should report the all-zeros NIGHT token type on its outputs', async (ctx: TestContext) => {
      ctx.task!.meta.custom = {
        labels: ['Query', 'Block', 'ByHeight', 'UnshieldedTokens', 'NIGHT'],
      };

      // An environment precondition, not indexer behaviour: a chain seeded with more than
      // one unshielded token type cannot say which of them is NIGHT.
      ctx.skip?.(
        genesisTokenTypes.length !== 1,
        `environment not provisioned: genesis carries ${genesisTokenTypes.length} unshielded token types, expected exactly one`,
      );

      expect(genesisTokenTypes[0]).toMatch(/^0{64}$/);
    });
  });

  // NIGHT-only by design, with no counterpart in the custom token suite: DUST is
  // generated by holding NIGHT, so a contract-minted custom unshielded token produces none
  // of the events below. The asymmetry between the two suites is correct — do not mirror
  // these two tests onto the custom token.
  describe('the dust activity of a confirmed NIGHT transaction', () => {
    /**
     * After an unshielded transaction is confirmed, the dust commitment Merkle tree should grow.
     * The dustCommitmentEndIndex of the transaction should be higher than the previous maximum.
     *
     * @given a confirmed NIGHT transaction
     * @when the transaction is queried from the indexer
     * @then its dustCommitmentEndIndex is greater than the highest one seen in the genesis block
     */
    test('should increase the dust commitment Merkle tree end index', async (ctx: TestContext) => {
      ctx.task!.meta.custom = {
        labels: [
          'Query',
          'Transaction',
          'Dust',
          'CommitmentMerkleTree',
          'UnshieldedTokens',
          'NIGHT',
        ],
      };

      ctx.skip?.(
        scenario.transactionResult.status !== 'confirmed',
        "Toolkit transaction hasn't been confirmed",
      );

      const transactionResponse = await scenario.httpClient.getTransactionByOffset({
        hash: scenario.transactionResult.txHash,
      });
      expect(transactionResponse).toBeSuccess();

      const transactions = transactionResponse.data!.transactions;
      const tx = transactions.find(
        (t: Transaction) => t.hash === scenario.transactionResult.txHash,
      );
      expect(tx).toBeDefined();

      const regularTx = tx as RegularTransaction;
      expect(regularTx.dustCommitmentEndIndex).toBeDefined();
      expect(regularTx.dustCommitmentEndIndex!).toBeGreaterThan(dustCommitmentEndIndexBeforeTx);

      log.debug(
        `dustCommitmentEndIndex before tx: ${dustCommitmentEndIndexBeforeTx}, after tx: ${regularTx.dustCommitmentEndIndex}`,
      );
    });

    /**
     * Once an unshielded transaction has been confirmed, the indexer should stream the full
     * sequence of DUST events associated with that transaction.
     *
     * @given a confirmed NIGHT transaction that produces DUST activity
     * @when dustLedgerEvents are subscribed from (previousMaxId + 1), so only the new events
     *       produced by this transaction are received
     * @then exactly three events are delivered, in the order DustGenerationDtimeUpdate,
     *       DustInitialUtxo, DustSpendProcessed
     */
    test('should deliver dust events in correct sequence after unshielded transaction', async (ctx: TestContext) => {
      ctx.task!.meta.custom = {
        labels: ['Subscription', 'Dust', 'UnshieldedTokens', 'NIGHT'],
      };

      const received = await collectValidDustLedgerEvents(
        scenario.wsClient,
        indexerEventCoordinator,
        3,
        previousMaxDustId + 1,
      );
      expect(received).toHaveLength(3);

      received.forEach((msg) => {
        const event = msg.data!.dustLedgerEvents;
        const parsed = DustLedgerEventsUnionSchema.safeParse(event);
        expect(
          parsed.success,
          `Schema error: ${JSON.stringify(parsed.error?.format(), null, 2)}`,
        ).toBe(true);
      });

      const eventTypes = received.map((msg) => msg.data!.dustLedgerEvents.__typename);
      expect(eventTypes).toEqual([
        'DustGenerationDtimeUpdate',
        'DustInitialUtxo',
        'DustSpendProcessed',
      ]);
    });
  });

  // NIGHT-only by design: the transfers below are submitted with the toolkit's default token,
  // and only the funding wallet holds enough NIGHT to fund two extra transfers mid-suite.
  //
  // `.sequential` documents that the A > B2 transfer depends on A > B1 having run first.
  // These tests run after the blocks above and deliberately do not clear the event buffers:
  // the shared transfer tests compare them against baselines captured in the scenario's
  // beforeAll, and matching on the submitted transaction hash already disambiguates the
  // streams.
  describe.sequential('a confirmed unshielded transfer streamed to address subscriptions', () => {
    /**
     * This test verifies correct propagation of event types across multi-destination subscriptions, ensuring that
     * the indexer only emits transaction data to the intended recipient while other wallets observe progress updates.
     *
     * @given a source wallet (A) and two destination wallets (B1, B2) all subscribed to unshielded transaction events
     * @when wallet A performs an unshielded transfer of 3 units to B1
     * @then B1 should receive a single `UnshieldedTransaction` event representing the received funds, while B2 should only
     * receive `UnshieldedTransactionsProgress` events and no actual `UnshieldedTransaction` payloads.
     */
    test('should emit UnshieldedTransaction only for the target wallet (A > B1)', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Wallet', 'Subscription', 'MultiDestination'] };

      const destinationAddress = scenario.wallet.destinations[0].destinationAddress;

      const b1TxResult = await scenario.toolkit.generateSingleTx(
        scenario.wallet.source.seed,
        'unshielded',
        destinationAddress,
        3,
      );

      // Wait for B1's UnshieldedTransaction matching the submitted tx hash
      const latestB1Tx = await retrySimple(async () => {
        const events = getEventsOfType(
          scenario.wallet.destinations[0].events,
          'UnshieldedTransaction',
        );
        return events.find((e) => e.transaction.hash === b1TxResult.txHash) ?? null;
      });

      // Wait for source event matching the same tx hash
      const latestSourceTx = await retrySimple(async () => {
        const events = getEventsOfType(scenario.wallet.source.events, 'UnshieldedTransaction');
        return events.find((e) => e.transaction.hash === b1TxResult.txHash) ?? null;
      });

      // Wait for B2 progress
      const latestB2Tx = await retrySimple(async () => {
        const progressEvents = getEventsOfType(
          scenario.wallet.destinations[1].events,
          'UnshieldedTransactionsProgress',
        );
        return progressEvents.at(-1) ?? null;
      });

      validateCrossWalletTransaction(
        [latestSourceTx],
        [latestB1Tx],
        scenario.wallet.source.address,
        destinationAddress,
        '3',
      );

      // Ensure B2 did not receive a UnshieldedTransaction event
      const b2Tx = getEventsOfType(scenario.wallet.destinations[1].events, 'UnshieldedTransaction');
      expect(b2Tx.length).toBe(0);

      // B2 must at least show progress
      expect(latestB2Tx).toBeDefined();
    });

    /**
     * This test validates correct event propagation when performing an unshielded transfer from wallet A to the second destination wallet (B2) in a multi-destination
     * subscription scenario.
     * @given a source wallet (A) and two destination wallets (B1, B2), all subscribed to unshielded transaction events
     * @when wallet A performs an unshielded transfer of 1 unit to B2
     * @then B2 should receive a single `UnshieldedTransaction` event representing the received funds, while B1 should only observe its own previous transaction history and must not receive the new `UnshieldedTransaction` intended for B2
     */
    test('should emit UnshieldedTransaction only for the target wallet (A > B2)', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Wallet', 'Subscription', 'MultiDestination'] };

      const secondDestinationAddress = scenario.wallet.destinations[1].destinationAddress;

      const b2TxResult = await scenario.toolkit.generateSingleTx(
        scenario.wallet.source.seed,
        'unshielded',
        secondDestinationAddress,
        1,
      );

      // Wait for B2's UnshieldedTransaction matching the submitted tx hash
      const latestB2Tx = await retrySimple(async () => {
        const b2Events = getEventsOfType(
          scenario.wallet.destinations[1].events,
          'UnshieldedTransaction',
        );
        return b2Events.find((e) => e.transaction.hash === b2TxResult.txHash) ?? null;
      });

      // B1 UnshieldedTransaction (should NOT match B2)
      const latestB1Tx = await retrySimple(async () => {
        const b1Events = getEventsOfType(
          scenario.wallet.destinations[0].events,
          'UnshieldedTransaction',
        );
        return b1Events.at(-1) ?? null;
      });

      // Source event matching the same tx hash
      const latestSourceTx = await retrySimple(async () => {
        const srcEvents = getEventsOfType(scenario.wallet.source.events, 'UnshieldedTransaction');
        return srcEvents.find((e) => e.transaction.hash === b2TxResult.txHash) ?? null;
      });

      validateCrossWalletTransaction(
        [latestSourceTx],
        [latestB2Tx],
        scenario.wallet.source.address,
        secondDestinationAddress,
        '1',
      );

      // Ensure B1 did NOT receive the B2 transaction
      expect(latestB1Tx.transaction.hash).not.toBe(latestB2Tx.transaction.hash);
    });
  });

  describe('an address with no transaction history', () => {
    /**
     * Validates event subscription behavior for an empty wallet.
     *
     * @given an empty wallet subscribed to unshielded transaction events
     * @when no transactions are performed
     * @then only ProgressUpdate events should be emitted by the indexer
     */
    test('should emit only ProgressUpdate for empty wallet', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Wallet', 'Subscription', 'EmptyWallet'] };

      const emptySeed = '000000000000000000000000000000000000000000000000000000000000000E';
      const emptyAddress = (await scenario.toolkit.showAddress(emptySeed)).unshielded;
      log.debug(`Empty wallet address: ${emptyAddress}`);

      const ws = new IndexerWsClient();
      await ws.connectionInit();
      const emptyEvents: UnshieldedTxSubscriptionResponse[] = [];

      const unsubscribe = ws.subscribeToUnshieldedTransactionEvents(
        {
          next: (e) => {
            emptyEvents.push(e);
          },
        },
        { address: emptyAddress },
      );

      try {
        const stabilized = await waitForEventsStabilization(emptyEvents, 1000);
        log.debug(`Received ${stabilized.length} events for empty wallet.`);

        // The stream must be alive, not merely silent: `every` on an empty array is vacuously true,
        // so without this an indexer emitting nothing at all would pass.
        expect(stabilized.length).toBeGreaterThan(0);

        const onlyProgressUpdates = stabilized.every((e) => {
          const data = e.data?.unshieldedTransactions;
          return (
            data?.__typename === 'UnshieldedTransactionsProgress' && data.highestTransactionId === 0
          );
        });

        expect(onlyProgressUpdates).toBe(true);
      } finally {
        unsubscribe();
        await ws.connectionClose();
      }
    });
  });
});
