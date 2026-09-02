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
import { IndexerHttpClient } from '@utils/indexer/http-client';
import type { Block, BlockOffset, RegularTransaction } from '@utils/indexer/indexer-types';
import {
  IndexerWsClient,
  SubscriptionHandlers,
  BlockSubscriptionResponse,
  GraphQLCompleteMessage,
} from '@utils/indexer/websocket-client';
import { buildErrorPayload } from '@utils/indexer/subscription-error';
import { EventCoordinator } from '@utils/event-coordinator';
import type { TestContext } from 'vitest';
import { BlockSchema } from '@utils/indexer/graphql/schema';

describe('block subscriptions', () => {
  let indexerWsClient: IndexerWsClient;
  let indexerHttpClient: IndexerHttpClient;
  let eventCoordinator: EventCoordinator;

  beforeEach(async () => {
    indexerHttpClient = new IndexerHttpClient();
    indexerWsClient = new IndexerWsClient();
    eventCoordinator = new EventCoordinator();
    await indexerWsClient.connectionInit();
  }, 30_000);

  afterEach(async () => {
    await indexerWsClient.connectionClose();
    eventCoordinator.clear();
  });

  /**
   * Helper to subscribe to block events and wait for a specific number of blocks.
   */
  async function collectBlocks(
    expectedCount: number,
    fromHeight?: number,
  ): Promise<BlockSubscriptionResponse[]> {
    // We wait until expected number of blocks has been recieved, as we want to make sure that
    // the subscription is working and we are receiving blocks
    const receivedBlocks: BlockSubscriptionResponse[] = [];
    const eventName = `${expectedCount} blocks received`;
    const blockSubscriptionHandler: SubscriptionHandlers<BlockSubscriptionResponse> = {
      next: (payload: BlockSubscriptionResponse) => {
        log.debug(`Received data:\n${JSON.stringify(payload)}`);
        receivedBlocks.push(payload);
        if (receivedBlocks.length === expectedCount) {
          eventCoordinator.notify(eventName);
          log.debug(`${expectedCount} blocks received`);
        }
      },
    };
    // If a starting height is provided, build a BlockOffset object using that height.
    // This will fetch the latest block and stream the new blocks as they are produced
    const blockOffset = fromHeight ? { height: fromHeight } : undefined;

    const unsubscribe = indexerWsClient.subscribeToBlockEvents(
      blockSubscriptionHandler,
      blockOffset,
    );

    // Blocks on MN are produced ~6s apart; the indexer processing adds a small
    // delay. For *live* subscriptions, `waitForAll` is measuring total time
    // from subscription start until `expectedCount` blocks have been received
    // — not the inter-block gap — so the budget must cover the worst-case
    // first-block latency (sub-WS-handshake + first block arrival) PLUS
    // (expectedCount-1) × block interval. Under healthy qanet a 2-block
    // collection completes in ~7s, but under load or while the indexer
    // catches up it can take 12-15s. 25s gives 2-3 block intervals of
    // headroom without masking a hung subscription.
    // Historical replays are instant — keep the small 5s grace.
    const blockStreamingBudgetMs = fromHeight ? 5_000 : 25_000;
    await eventCoordinator.waitForAll([eventName], blockStreamingBudgetMs);

    unsubscribe();
    return receivedBlocks;
  }

  describe('a subscription to block updates without parameters', () => {
    /**
     * Subscribing to block updates without any offset parameters should stream
     * blocks starting from the latest block and continue streaming new blocks
     * as they are produced.
     *
     * @given no block offset parameters are provided
     * @when we subscribe to block events
     * @then we should receive blocks starting from the latest block
     * @and we should receive new blocks as they are produced
     */
    test('should stream blocks starting from the latest block', async () => {
      const receivedBlocks = await collectBlocks(2);

      // In 6 seconds window we should have received at
      // least 1 block, maybe 2 but no more than that
      expect(receivedBlocks.length).toBe(2);
    });

    /**
     * Validates that all streamed blocks conform to the expected schema.
     *
     * @given a block subscription without any offset parameters
     * @when blocks are streamed from the indexer in real time
     * @then each received block should match the BlockSchema definition
     */
    test('should stream blocks adhering to the expected schema', async () => {
      const latestResponse = await indexerHttpClient.getLatestBlock();
      expect(latestResponse).toBeSuccess();

      const latestHeight = latestResponse.data?.block?.height;
      expect(latestHeight).toBeDefined();

      if (!latestHeight) {
        throw new Error('latestHeight is undefined');
      }

      const startHeight = latestHeight - 10;
      //Stream a set of historical blocks from that height
      const receivedBlocks = await collectBlocks(5, startHeight);
      receivedBlocks
        .filter((msg) => msg?.data?.blocks)
        .forEach((msg) => {
          expect.soft(msg).toBeSuccess();
          const parsed = BlockSchema.safeParse(msg.data?.blocks);
          expect(
            parsed.success,
            `Block subscription schema validation failed: ${JSON.stringify(parsed.error?.format(), null, 2)}`,
          ).toBe(true);
        });
    });
  });

  describe('a subscription to block updates by hash', () => {
    /**
     * Subscribing to block updates with a valid block hash should stream
     * blocks starting from the specified block and continue streaming
     * subsequent blocks.
     *
     * @given a valid block hash that exists in the blockchain
     * @when we subscribe to block events with that hash as offset
     * @then we should receive blocks starting from the block with that hash
     */
    test('should stream blocks starting from the block with that hash, given that hash exists', async () => {
      // Let's get the hash of the genesis block ...
      const genesisBlockResponse = await indexerHttpClient.getBlockByOffset({
        height: 0,
      });
      expect(genesisBlockResponse).toBeSuccess();
      const genesisBlock = genesisBlockResponse.data?.block;
      expect(genesisBlock).toBeDefined();

      //... and extract its hash
      const knownHash = genesisBlock?.hash;
      expect(knownHash).toBeDefined();
      const blockOffset: BlockOffset = {
        hash: knownHash,
      };

      const messagesReceived: BlockSubscriptionResponse[] = [];
      const blockSubscriptionHandler: SubscriptionHandlers<BlockSubscriptionResponse> = {
        next: (payload: BlockSubscriptionResponse) => {
          log.debug(`Received data: ${JSON.stringify(payload)}`);
          messagesReceived.push(payload);
          if (messagesReceived.length === 10) {
            eventCoordinator.notify('expectedBlocksReceived');
            log.debug('Expected # of blocks received');
          }
        },
        error: (err) => {
          const synthetic = buildErrorPayload<BlockSubscriptionResponse>(err);
          log.error(`Unexpected error frame: ${JSON.stringify(synthetic)}`);
          messagesReceived.push(synthetic);
          eventCoordinator.notify('error');
        },
      };

      const unsubscribe = indexerWsClient.subscribeToBlockEvents(
        blockSubscriptionHandler,
        blockOffset,
      );

      await eventCoordinator.waitForAny(['expectedBlocksReceived', 'error']);

      unsubscribe();

      // Even if after we received the expected number of blocks, we unsubscribe,
      // we might receive more blocks due to race conditions, so we expect at least 10
      // blocks to be received
      expect(messagesReceived.length).toBeGreaterThanOrEqual(10);
      for (const msg of messagesReceived) {
        expect.soft(msg).toBeSuccess();
        expect.soft(msg.data?.blocks).toBeDefined();
      }

      const firstBlockStreamed = messagesReceived[0].data?.blocks;
      expect(firstBlockStreamed).toBeDefined();
      expect(firstBlockStreamed?.hash).toBe(blockOffset.hash);
    });

    /**
     * Subscribing to block updates with a non-existent block hash should
     * not stream any blocks and should return an error response indicating
     * that the block was not found.
     *
     * @given a block hash that does not exist on chain
     * @when we subscribe to block events with that hash as offset
     * @then we should receive an error message indicating the block was not found
     */
    test(`should return an error message, given that hash doesn't exist`, async () => {
      const blockOffset: BlockOffset = {
        hash: '0000000000000000000000000000000000000000000000000000000000000000',
      };

      let completionMessage: GraphQLCompleteMessage | null = null;
      const messagesReceived: BlockSubscriptionResponse[] = [];

      const blockSubscriptionHandler: SubscriptionHandlers<BlockSubscriptionResponse> = {
        next: (payload: BlockSubscriptionResponse) => {
          log.debug(`Received data: ${JSON.stringify(payload)}`);
          messagesReceived.push(payload);
        },
        error: (err) => {
          const synthetic = buildErrorPayload<BlockSubscriptionResponse>(err);
          log.debug(`Received error frame: ${JSON.stringify(synthetic)}`);
          messagesReceived.push(synthetic);
          eventCoordinator.notify('error');
        },
        complete: (message) => {
          log.debug(`Complete message: ${JSON.stringify(message)}`);
          completionMessage = message;
          eventCoordinator.notify('completion');
        },
      };

      indexerWsClient.subscribeToBlockEvents(blockSubscriptionHandler, blockOffset);

      await eventCoordinator.waitForAll(['error', 'completion']);

      // Validate that we received a complete message
      expect(completionMessage).toBeDefined();
      expect(completionMessage!.type).toBe('complete');
      expect(completionMessage!.id).toBeDefined();

      // ... only one message should be received
      expect(messagesReceived.length).toBe(1);
      expect(messagesReceived[0]).toBeError();
      const errorMessage = messagesReceived[0].errors?.[0].message;
      expect(errorMessage).toContain(`block with hash`);
      expect(errorMessage).toContain(`${blockOffset.hash}`);
      expect(errorMessage).toContain(`not found`);
    });

    /**
     * Subscribing to block updates with an invalid block hash should
     * not stream any blocks and should return an error response indicating
     * that the block hash is invalid.
     *
     * @given an invalid block hash
     * @when we subscribe to block events with that hash as offset
     * @then we should receive an error message indicating the block hash is invalid
     */
    test(`should return an error message, given that hash is invalid`, async () => {
      const blockOffset: BlockOffset = {
        hash: 'TT',
      };

      let completionMessage: GraphQLCompleteMessage | null = null;
      const messagesReceived: BlockSubscriptionResponse[] = [];

      const blockSubscriptionHandler: SubscriptionHandlers<BlockSubscriptionResponse> = {
        next: (payload: BlockSubscriptionResponse) => {
          log.debug(`Received data: ${JSON.stringify(payload)}`);
          messagesReceived.push(payload);
        },
        error: (err) => {
          const synthetic = buildErrorPayload<BlockSubscriptionResponse>(err);
          log.debug(`Received error frame: ${JSON.stringify(synthetic)}`);
          messagesReceived.push(synthetic);
          eventCoordinator.notify('error');
        },
        complete: (message) => {
          log.debug(`Complete message: ${JSON.stringify(message)}`);
          completionMessage = message;
          eventCoordinator.notify('completion');
        },
      };

      indexerWsClient.subscribeToBlockEvents(blockSubscriptionHandler, blockOffset);

      await eventCoordinator.waitForAll(['error', 'completion']);

      // Validate that we received a complete message
      expect(completionMessage).toBeDefined();
      expect(completionMessage!.type).toBe('complete');
      expect(completionMessage!.id).toBeDefined();

      // ... only one message should be received
      expect(messagesReceived.length).toBe(1);
      expect(messagesReceived[0]).toBeError();
      const errorMessage = messagesReceived[0].errors?.[0].message;
      expect(errorMessage).toContain(`invalid block hash`);
    });
  });

  describe('a subscription to block updates by height', () => {
    /**
     * Subscribing to block updates with a valid block height should stream
     * blocks starting from the specified block height and continue streaming
     * subsequent blocks.
     *
     * @given we get the latest block height from indexer
     * @when we subscribe to block events with that height - 20 as offset
     * @then we should receive blocks starting from the block with that height
     * @and the first block received should have the requested height
     * @and we should receive subsequent blocks as they are produced
     */
    test('should stream blocks from the block with that height, given that height exists', async ({
      skip,
    }: TestContext) => {
      const latestBlockResponse = await indexerHttpClient.getLatestBlock();
      expect(latestBlockResponse).toBeSuccess();
      const latestBlock = latestBlockResponse.data?.block;
      expect(latestBlock).toBeDefined();

      // We need to decide a number of blocks to receive, after which we are
      // happy with the test. Say that number is 20.
      // So we need to make sure there are at least 20 blocks on chain, if not
      // we skip the test becausee the precondition is not met
      if (latestBlock?.height && latestBlock?.height < 20) {
        skip?.(true, 'Skipping as we want at least 20 blocks to be produced');
      }

      const blockMessagesReceived: BlockSubscriptionResponse[] = [];

      const blockHeight = latestBlock?.height;
      expect(blockHeight).toBeDefined();
      const blockOffset: BlockOffset = { height: blockHeight! - 20 };

      const blockSubscriptionHandler: SubscriptionHandlers<BlockSubscriptionResponse> = {
        next: (payload) => {
          blockMessagesReceived.push(payload);
          log.debug(`Received data: ${JSON.stringify(payload)}`);
          if (blockMessagesReceived.length === 20) {
            log.debug('Stop receiving blocks');
            eventCoordinator.notify('expectedBlocksReceived');
          }
        },
      };

      const unsubscribe = indexerWsClient.subscribeToBlockEvents(
        blockSubscriptionHandler,
        blockOffset,
      );

      await eventCoordinator.waitFor('expectedBlocksReceived');

      unsubscribe();

      // We ask for 20 blocks but due to race conditions we might receive more depending on who is faster...
      // ... the test unscribing or the indexer sending blocks
      expect(blockMessagesReceived.length).toBeGreaterThanOrEqual(20);
      expect(blockMessagesReceived[0]).toBeSuccess();
      expect((blockMessagesReceived[0].data as { blocks: Block }).blocks.height).toBe(
        blockOffset.height,
      );
    });

    /**
     * Subscribing to block updates with a block height higher than the latest
     * block height should not stream any blocks and should return an error
     * response indicating that the block was not found.
     *
     * @given a block height that is higher than the latest block height
     * @when we subscribe to block events with that height as offset
     * @then we should not receive any blocks
     * @and we should receive an error indicating that the block was not found
     */
    test('should return an error message, given that height is higher than the latest block height', async () => {
      const latestBlockResponse = await indexerHttpClient.getLatestBlock();

      expect(latestBlockResponse).toBeSuccess();
      const latestBlock = latestBlockResponse.data?.block;
      expect(latestBlock).toBeDefined();
      expect(latestBlock?.height).toBeDefined();
      const blockHeight = latestBlock?.height;
      expect(blockHeight).toBeDefined();

      // We need to make sure that the block height is higher than the latest block height
      // so we add 10 to the latest block height
      const blockOffset: BlockOffset = { height: blockHeight! + 10 };

      const blockMessagesReceived: BlockSubscriptionResponse[] = [];

      const blockSubscriptionHandler: SubscriptionHandlers<BlockSubscriptionResponse> = {
        next: (payload) => {
          blockMessagesReceived.push(payload);
          log.debug(`Received data: ${JSON.stringify(payload)}`);
        },
        error: (err) => {
          const synthetic = buildErrorPayload<BlockSubscriptionResponse>(err);
          log.debug(`Received error frame: ${JSON.stringify(synthetic)}`);
          blockMessagesReceived.push(synthetic);
          eventCoordinator.notify('error');
        },
      };

      indexerWsClient.subscribeToBlockEvents(blockSubscriptionHandler, blockOffset);

      await eventCoordinator.waitFor('error');

      expect(blockMessagesReceived.length).toBe(1);
      expect(blockMessagesReceived[0]).toBeError();
      const errorMessage = blockMessagesReceived[0].errors?.[0].message;
      expect(errorMessage).toContain(`block with height`);
      expect(errorMessage).toContain(`${blockOffset.height}`);
      expect(errorMessage).toContain(`not found`);
    });

    /**
     * Subscribing to block updates with an invalid block height should
     * not stream any blocks and should return an error response indicating
     * that the block height is invalid.
     *
     * @given an invalid block height
     * @when we subscribe to block events with that height as offset
     * @then we should not receive any blocks
     * @and we should receive an error indicating that the block height is invalid
     */
    test('should return an error message, given that height is invalid', async () => {
      const blockOffset: BlockOffset = { height: 2 ** 32 };
      const blockMessagesReceived: BlockSubscriptionResponse[] = [];

      let completionMessage: GraphQLCompleteMessage | null = null;

      const blockSubscriptionHandler: SubscriptionHandlers<BlockSubscriptionResponse> = {
        next: (payload) => {
          blockMessagesReceived.push(payload);
          log.debug(`Received data: ${JSON.stringify(payload)}`);
        },
        error: (err) => {
          const synthetic = buildErrorPayload<BlockSubscriptionResponse>(err);
          log.debug(`Received error frame: ${JSON.stringify(synthetic)}`);
          blockMessagesReceived.push(synthetic);
          eventCoordinator.notify('error');
        },
        complete: (message) => {
          log.debug(`Complete message: ${JSON.stringify(message)}`);
          eventCoordinator.notify('completion');
          completionMessage = message;
        },
      };

      indexerWsClient.subscribeToBlockEvents(blockSubscriptionHandler, blockOffset);

      await eventCoordinator.waitForAll(['error', 'completion']);

      expect(completionMessage).toBeDefined();
      expect(completionMessage!.type).toBe('complete');
      expect(completionMessage!.id).toBeDefined();

      expect(blockMessagesReceived.length).toBe(1);
      expect(blockMessagesReceived[0]).toBeError();
      const errorMessage = blockMessagesReceived[0].errors?.[0].message;
      expect(errorMessage).toContain(`Failed to parse "Int"`);
      expect(errorMessage).toContain(`Only integers from 0 to 4294967295 are accepted`);
    });
  });

  describe('a subscription to block updates by height and hash', () => {
    /**
     * Subscribing to block updates with a valid block height and hash should
     * return an error message, as only one parameter at a time can be used
     *
     * @given a valid block height and hash
     * @when we subscribe to block events with that height and hash as offset
     * @then we should receive an error message indicating that only one parameter at a time can be used
     */
    test('should return an error message, as only one parameter at a time can be used', async () => {
      const blockOffset: BlockOffset = {
        height: 1,
        hash: '0'.repeat(64),
      };

      const blockMessagesReceived: BlockSubscriptionResponse[] = [];

      let completionMessage: GraphQLCompleteMessage | null = null;

      const blockSubscriptionHandler: SubscriptionHandlers<BlockSubscriptionResponse> = {
        next: (payload) => {
          blockMessagesReceived.push(payload);
          log.debug(`Received data: ${JSON.stringify(payload)}`);
        },
        error: (err) => {
          const synthetic = buildErrorPayload<BlockSubscriptionResponse>(err);
          log.debug(`Received error frame: ${JSON.stringify(synthetic)}`);
          blockMessagesReceived.push(synthetic);
          eventCoordinator.notify('error');
        },
        complete: (message) => {
          log.debug(`Complete message: ${JSON.stringify(message)}`);
          eventCoordinator.notify('completion');
          completionMessage = message;
        },
      };

      indexerWsClient.subscribeToBlockEvents(blockSubscriptionHandler, blockOffset);

      await eventCoordinator.waitForAll(['error', 'completion']);

      expect(completionMessage).toBeDefined();
      expect(completionMessage!.type).toBe('complete');
      expect(completionMessage!.id).toBeDefined();

      expect(blockMessagesReceived.length).toBe(1);
      expect(blockMessagesReceived[0]).toBeError();
      const errorMessage = blockMessagesReceived[0].errors?.[0].message;
      expect(errorMessage).toContain(`Invalid value for argument`);
      expect(errorMessage).toContain(`Oneof input objects requires have exactly one field`);
    });
  });

  /**
   * This describe validates `RegularTransaction.fees` semantics; the block subscription is
   * the transport, not the system under test.
   *
   * Regression coverage for #1068 (release/4.0 backport of #1031 + #1061). #1031 routes
   * fee computation through the ledger's `Transaction::fees()` API and populates both
   * `paidFees` and `estimatedFees`. #1061 fixes a `clamp_and_normalize` bug in
   * `post_block_update` that, on long-running chains, drove `fee_prices.overall_price`
   * down toward `MIN_COST = 100` over many blocks. Combined symptom on mainnet pre-fix:
   * every regular transaction reported `paidFees == 1 SPECK`. Both PRs must travel
   * together — #1031 alone makes the underlying drift visible; #1061 alone is invisible.
   *
   * Strategy: scan a recent slice of historical blocks via offset replay (instant on
   * the wire, not chain-paced), filter to RegularTransactions with a `fees` payload,
   * and assert per-tx and population invariants. Avoids dependence on test-env wallet
   * data, which is reset and varies per environment.
   */
  describe('regular transaction fees', () => {
    // Slim subscription document scoped to this test: drops zswapLedgerEvents,
    // dustLedgerEvents, contractActions, and other heavy fields we don't need.
    // We only need __typename, hash, the new top-level RegularTransaction.fee
    // (added in PR #1036 / issue #1032), and the deprecated fees wrapper.
    // Substantially reduces per-block payload size and replay time.
    const SLIM_BLOCKS_SUBSCRIPTION = `subscription BlocksSubscriptionFromBlockByOffset($OFFSET: BlockOffset) {
      blocks(offset: $OFFSET) {
        hash
        height
        transactions {
          hash
          __typename
          ... on RegularTransaction {
            fee
            fees {
              paidFees
              estimatedFees
            }
          }
        }
      }
    }`;

    /**
     * @given a chain with regular transactions in the recent ~50000-block window
     *        (wide enough to find a sample on sparse test envs like devnet)
     * @when we collect a sample of regular transactions via block subscription replay
     * @then every sampled tx returns paidFees and estimatedFees in SPECKs (>= 1, < 10^9),
     *       and paidFees == estimatedFees per chain-indexer/src/domain/ledger_state.rs
     *       (both populated from the same Transaction::fees(params, true) call).
     *       A weight-magnitude value (~10^10) would indicate a 4.0.1 regression — that's
     *       the failure mode this test catches.
     *
     *       Also asserts the post-#1036 invariant for issue #1032: the new
     *       canonical `fee` field on RegularTransaction returns the same
     *       SPECK value as the deprecated `fees.paidFees`.
     */
    test.skip('should report ledger paidFees and estimatedFees on regular transactions', async (ctx: TestContext) => {
      ctx.task!.meta.custom = {
        labels: ['Subscription', 'Block', 'Transaction', 'Fees', 'Regression'],
      };

      const SAMPLE_TARGET = 10;
      const HISTORY_WINDOW_BLOCKS = 50000;
      const MAX_WAIT_MS = 60_000;

      const latestResponse = await indexerHttpClient.getLatestBlock();
      expect(latestResponse).toBeSuccess();
      const latestHeight = latestResponse.data?.block?.height;
      expect(latestHeight).toBeDefined();
      log.debug(`Latest height: ${latestHeight}`);

      // Start from a recent slice. On a freshly-reset env (few hundred blocks) this
      // collapses to height 1; on a long-running env it caps the replay span so we
      // don't scan the entire chain. Recent blocks are the right witness either way:
      // the #1061 drift bug accumulates over time, so it surfaces in current state.
      const startHeight = Math.max(1, latestHeight! - HISTORY_WINDOW_BLOCKS);
      log.debug(`Start height: ${startHeight}`);
      const collected: {
        fee: bigint;
        paidFees: bigint;
        estimatedFees: bigint;
        hash?: string;
      }[] = [];
      const sampleReadyEvent = `${SAMPLE_TARGET} regular transactions collected`;

      const handler: SubscriptionHandlers<BlockSubscriptionResponse> = {
        next: (payload: BlockSubscriptionResponse) => {
          const block = payload.data?.blocks;
          if (!block?.transactions || collected.length >= SAMPLE_TARGET) return;
          for (const tx of block.transactions) {
            if (tx.__typename !== 'RegularTransaction') continue;
            const reg = tx as RegularTransaction;
            if (reg.fees == null || reg.fee == null) continue;
            collected.push({
              fee: BigInt(reg.fee),
              paidFees: BigInt(reg.fees.paidFees),
              estimatedFees: BigInt(reg.fees.estimatedFees),
              hash: reg.hash,
            });
            if (collected.length >= SAMPLE_TARGET) {
              eventCoordinator.notify(sampleReadyEvent);
              indexerWsClient.send<GraphQLCompleteMessage>({ id: '1', type: 'complete' });
              return;
            }
          }
        },
      };

      log.debug(`Subscribing to block events from height: ${startHeight}`);

      const unsubscribe = indexerWsClient.subscribeToBlockEvents(
        handler,
        { height: startHeight },
        SLIM_BLOCKS_SUBSCRIPTION,
      );

      try {
        await eventCoordinator.waitForAll([sampleReadyEvent], MAX_WAIT_MS);
      } catch {
        // Timed out before reaching SAMPLE_TARGET; assert on whatever we did collect.
        log.debug(
          `Timed out after ${MAX_WAIT_MS}ms with ${collected.length} regular transactions collected`,
        );
      }
      unsubscribe();

      if (collected.length === 0) {
        ctx.skip?.(
          true,
          `no regular transactions found in blocks ${startHeight}..${latestHeight} on this env`,
        );
        return;
      }

      log.debug(`Validating fees on ${collected.length} regular transactions`);

      const SPECK_UPPER_BOUND = 1_000_000_000n; // 10^9 SPECKs. Substrate-weight regressions land at 10^10-10^11.

      for (const t of collected) {
        expect
          .soft(
            t.paidFees,
            `paidFees < 1 SPECK; expected at least the floor (tx ${t.hash}, see #1068)`,
          )
          .toBeGreaterThanOrEqual(1n);
        expect
          .soft(
            t.paidFees,
            `paidFees >= 10^9 SPECKs; weight-magnitude value suggests 4.0.1 regression (tx ${t.hash}, see #1068)`,
          )
          .toBeLessThan(SPECK_UPPER_BOUND);

        expect
          .soft(
            t.estimatedFees,
            `estimatedFees < 1 SPECK; expected at least the floor (tx ${t.hash})`,
          )
          .toBeGreaterThanOrEqual(1n);
        expect
          .soft(
            t.estimatedFees,
            `estimatedFees >= 10^9 SPECKs; weight-magnitude value suggests 4.0.1 regression (tx ${t.hash})`,
          )
          .toBeLessThan(SPECK_UPPER_BOUND);

        // Post-#1031 invariant: paid_fees and estimated_fees come from the same
        // Transaction::fees(params, true) call in chain-indexer/src/domain/ledger_state.rs.
        // Holds on every fix-bearing version (4.0.2, 4.2.x, 4.3.x).
        expect.soft(t.paidFees, `paidFees != estimatedFees (tx ${t.hash})`).toBe(t.estimatedFees);

        // Post-#1036 invariant (issue #1032): the new top-level `fee` field is
        // the canonical replacement for the deprecated `fees` wrapper, and
        // returns the same SPECK value as `fees.paidFees`. This locks in the
        // equivalence so a future regression that diverges them is caught.
        expect.soft(t.fee, `fee != paidFees (tx ${t.hash}, see #1032)`).toBe(t.paidFees);
      }
    }, 50_000);
  });
});
