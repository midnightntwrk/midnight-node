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
import { bech32m } from 'bech32';
import { Buffer } from 'node:buffer';
import '@utils/logging/test-logging-hooks';
import type { TestContext } from 'vitest';
import {
  IndexerWsClient,
  DustGenerationsSubscriptionResponse,
} from '@utils/indexer/websocket-client';
import { extractSubscriptionErrorMessage } from '@utils/indexer/subscription-error';
import { isBlockHashDustGenerationsSupported } from '@utils/indexer/schema-feature-probe';
import { DustGenerationsEventSchema } from '@utils/indexer/graphql/schema';
import { IndexerHttpClient } from '@utils/indexer/http-client';
import { env } from 'environment/model';
import dataProvider from '@utils/testdata-provider';

const indexerHttpClient = new IndexerHttpClient();

// Every case in this file sends the block-hash subscription document, which fails
// GraphQL validation outright on an environment still serving the older
// startIndex/endIndex signature. Probed once, then each test skips rather than
// reporting a missing surface as a behavioural failure.
let blockHashSurfacePresent = false;
const SURFACE_ABSENT_REASON =
  'deployed indexer does not serve the block-hash dustGenerations signature';

function encodeDustAddressAsHex(dustAddress: string): string {
  const { words } = bech32m.decode(dustAddress);
  return Buffer.from(bech32m.fromWords(words)).toString('hex');
}

function generateDustAddressForNetworkId(networkId: string): string {
  const hrp = networkId === 'mainnet' ? 'mn_dust' : `mn_dust_${networkId}`;
  const payload = Buffer.alloc(32, 1);
  return bech32m.encode(hrp, bech32m.toWords(payload));
}

function safeUnsubscribe(unsubscribe: () => void): void {
  try {
    unsubscribe();
  } catch (error) {
    // If the websocket is already closed during teardown, unsubscribe can throw.
    log.debug(`Ignoring unsubscribe error during teardown: ${String(error)}`);
  }
}

/**
 * Resolves the dust address a wallet registered for the given Cardano reward address.
 */
async function fetchDustAddress(rewardAddress: string): Promise<string> {
  const generationsResponse = await indexerHttpClient.getDustGenerations([rewardAddress]);
  expect(generationsResponse).toBeSuccess();
  const generations = generationsResponse.data!.dustGenerations;
  expect(generations.length).toBeGreaterThanOrEqual(1);
  expect(generations[0].registrations.length).toBeGreaterThanOrEqual(1);
  return generations[0].registrations[0].dustAddress;
}

/**
 * Fetches the block the subscription snapshot is pinned to.
 */
async function fetchBlock(offset?: {
  height: number;
}): Promise<{ hash: string; height: number; dustGenerationEndIndex: number }> {
  const response = offset
    ? await indexerHttpClient.getBlockByOffset(offset)
    : await indexerHttpClient.getLatestBlock();
  expect(response).toBeSuccess();
  const block = response.data!.block!;
  return {
    hash: block.hash,
    height: block.height,
    dustGenerationEndIndex: block.dustGenerationEndIndex!,
  };
}

interface DustGenerationsSubscriptionArgs {
  dustAddress: string;
  blockHash: string;
  dtimeCutoffHeight: number;
}

/**
 * Subscribes to dustGenerations and collects every event until the server completes
 * the subscription. The block-hash-scoped subscription is finite by design, so
 * completion is the expected terminal signal; an error or a timeout rejects.
 */
function collectDustGenerations(
  wsClient: IndexerWsClient,
  args: DustGenerationsSubscriptionArgs,
  timeoutMs = 30_000,
): Promise<DustGenerationsSubscriptionResponse[]> {
  return new Promise((resolve, reject) => {
    const events: DustGenerationsSubscriptionResponse[] = [];
    let settled = false;
    let unsubscribe = () => {};
    const settle = (handler: () => void) => {
      if (settled) return;
      settled = true;
      clearTimeout(timeout);
      handler();
    };

    const timeout = setTimeout(() => {
      safeUnsubscribe(unsubscribe);
      settle(() =>
        reject(
          new Error(
            `Timed out after ${timeoutMs}ms waiting for the dust generations subscription ` +
              `to complete (received ${events.length} events)`,
          ),
        ),
      );
    }, timeoutMs);

    const subscription = wsClient.subscribeToDustGenerations(
      {
        next: (payload) => {
          events.push(payload);
        },
        error: (error) => {
          safeUnsubscribe(unsubscribe);
          settle(() =>
            reject(new Error(`Subscription error: ${extractSubscriptionErrorMessage(error)}`)),
          );
        },
        complete: () => {
          settle(() => resolve(events));
        },
      },
      args.dustAddress,
      args.blockHash,
      args.dtimeCutoffHeight,
    );
    unsubscribe = subscription.unsubscribe;
  });
}

// Rejection reason when the server accepted the subscription instead of failing it.
// Tests that gate on a server-side guard being present branch on this.
const COMPLETED_WITHOUT_ERROR = 'Subscription completed without error';

/**
 * Subscribes to dustGenerations and resolves with the subscription error message.
 * Completion without an error, or a timeout, rejects.
 */
function collectDustGenerationsError(
  wsClient: IndexerWsClient,
  args: DustGenerationsSubscriptionArgs,
  timeoutMs = 10_000,
): Promise<string> {
  return new Promise((resolve, reject) => {
    let unsubscribe = () => {};
    const timeout = setTimeout(() => {
      safeUnsubscribe(unsubscribe);
      reject(new Error('Timed out waiting for a subscription error'));
    }, timeoutMs);

    const subscription = wsClient.subscribeToDustGenerations(
      {
        error: (error) => {
          clearTimeout(timeout);
          safeUnsubscribe(unsubscribe);
          resolve(extractSubscriptionErrorMessage(error));
        },
        complete: () => {
          clearTimeout(timeout);
          reject(new Error(COMPLETED_WITHOUT_ERROR));
        },
      },
      args.dustAddress,
      args.blockHash,
      args.dtimeCutoffHeight,
    );
    unsubscribe = subscription.unsubscribe;
  });
}

function assertEventsMatchSchema(events: DustGenerationsSubscriptionResponse[]): void {
  for (const msg of events) {
    expect(msg).toBeSuccess();
    const event = msg.data!.dustGenerations;
    const parsed = DustGenerationsEventSchema.safeParse(event);
    expect(
      parsed.success,
      `Dust generations event schema validation failed: ${JSON.stringify(parsed.error, null, 2)}`,
    ).toBe(true);
  }
}

function eventsOfType(
  events: DustGenerationsSubscriptionResponse[],
  typename: string,
): DustGenerationsSubscriptionResponse[] {
  return events.filter((msg) => msg.data?.dustGenerations?.__typename === typename);
}

type PinnedBlock = Awaited<ReturnType<typeof fetchBlock>>;

// The resolver's rejection for a snapshot older than `max_snapshot_age`.
const FRESHNESS_REJECTION = 'older than the snapshot freshness window';

// The indexer rejects snapshots older than its `max_snapshot_age` (500 by
// default) against the tip *at subscribe time*, not at block-selection time.
// Starting 400 blocks back leaves ~100 blocks (~10 minutes at 6s blocks) of
// margin against the window sliding mid-test. That margin only exists while the
// window is configured above ~410; below that the server rejects this offset, and
// the test steps closer to the tip rather than reporting configuration as a defect.
const IN_WINDOW_OFFSET = 400;

// Floor for stepping closer to the tip: below this there is too little room left
// for a snapshot comparison to be worth making.
const MIN_IN_WINDOW_OFFSET = 25;

// Far enough outside the window to survive it being raised.
const OUT_OF_WINDOW_OFFSET = 5_000;
const OUT_OF_WINDOW_MIN_MARGIN = 1_000;

/**
 * Opens a block-pinned subscription and asserts the activity-independent
 * invariants: every event is schema-valid, exactly one progress event terminates
 * the snapshot, and its highestIndex is the queried block's tree size. Returns
 * that highestIndex so callers can compare snapshots across blocks.
 */
async function assertPinnedSnapshot(
  wsClient: IndexerWsClient,
  dustAddress: string,
  block: PinnedBlock,
): Promise<number> {
  const events = await collectDustGenerations(wsClient, {
    dustAddress,
    blockHash: block.hash,
    dtimeCutoffHeight: 0,
  });

  assertEventsMatchSchema(events);
  const progressEvents = eventsOfType(events, 'DustGenerationsProgress');
  expect(progressEvents).toHaveLength(1);
  const { highestIndex } = progressEvents[0].data!.dustGenerations as { highestIndex: number };
  log.debug(
    `Snapshot pinned to block ${block.height}: endIndex ${block.dustGenerationEndIndex}, ` +
      `highestIndex ${highestIndex}, ${events.length} event(s)`,
  );
  expect(
    highestIndex,
    `highestIndex at block ${block.height} should reflect that block's tree size`,
  ).toBe(block.dustGenerationEndIndex - 1);
  return highestIndex;
}

type PinnedSnapshotOutcome = { highestIndex: number } | { rejection: string };

/**
 * Runs the per-block assertions unless the server refuses the block as older than
 * its snapshot freshness window, in which case the rejection message is returned
 * so the caller can decide what that means in its context. `max_snapshot_age` is
 * runtime configuration the schema does not expose, so this rejection is the only
 * signal that the deployed window is narrower than a caller assumed. Every other
 * failure propagates.
 */
async function assertPinnedSnapshotUnlessStale(
  wsClient: IndexerWsClient,
  dustAddress: string,
  block: PinnedBlock,
): Promise<PinnedSnapshotOutcome> {
  try {
    return { highestIndex: await assertPinnedSnapshot(wsClient, dustAddress, block) };
  } catch (error) {
    const message = extractSubscriptionErrorMessage(error);
    if (message.includes(FRESHNESS_REJECTION)) {
      return { rejection: message };
    }
    throw error;
  }
}

/**
 * Skips the current test, recording the reason in the run log as well as the
 * report so a reader of either can tell why coverage was reduced.
 */
function skipWithReason(ctx: TestContext, reason: string): void {
  log.warn(reason);
  ctx.skip(true, reason);
}

/**
 * Locates the newest block inside `(older, newer]` at which the generation tree
 * grew — the lowest height whose dustGenerationEndIndex already equals `newer`'s —
 * and returns the tight pair straddling it. Picking the newest rather than the
 * oldest boundary in the range maximises the margin against the sliding freshness
 * window. Callers must have established that the two bounding blocks differ.
 */
async function findNewestGrowthBoundary(
  older: PinnedBlock,
  newer: PinnedBlock,
): Promise<{ before: PinnedBlock; after: PinnedBlock }> {
  let low = older.height; // known to have a smaller endIndex than `newer`
  let high = newer.height; // known to have `newer`'s endIndex

  while (high - low > 1) {
    const mid = Math.floor((low + high) / 2);
    const block = await fetchBlock({ height: mid });
    if (block.dustGenerationEndIndex === newer.dustGenerationEndIndex) {
      high = mid;
    } else {
      low = mid;
    }
  }

  return { before: await fetchBlock({ height: low }), after: await fetchBlock({ height: high }) };
}

// Dust generation registrations require a Cardano-side mapping which has no
// counterpart in the `undeployed` environment. Skip the whole surface there;
// re-enable once #1152 lands local Cardano test-data provisioning.
describe.skipIf(env.isUndeployedEnv())('dust generations subscription', () => {
  let indexerWsClient: IndexerWsClient;

  beforeAll(async () => {
    blockHashSurfacePresent = await isBlockHashDustGenerationsSupported();
    if (!blockHashSurfacePresent) {
      log.warn(
        `dustGenerations(blockHash) is absent on ${env.getCurrentEnvironmentName()}; ` +
          'skipping the whole surface',
      );
    }
  }, 30_000);

  beforeEach(async () => {
    indexerWsClient = new IndexerWsClient();
    await indexerWsClient.connectionInit();
  }, 30_000);

  afterEach(async () => {
    await indexerWsClient.connectionClose();
  });

  describe('a subscription at the latest block hash', () => {
    /**
     * A dust generations subscription streams a finite snapshot and completes.
     *
     * @given a registered dust address and the latest block hash
     * @when a dustGenerations subscription is opened at that block with dtimeCutoffHeight 0
     * @then generation events are streamed, the last event is a DustGenerationsProgress,
     *       and the subscription completes on its own
     * @and each event matches the expected schema
     */
    test('should stream a complete generation snapshot for a registered dust address', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Subscription', 'Dust', 'Generations'] };
      if (!blockHashSurfacePresent) return ctx.skip(true, SURFACE_ABSENT_REASON);

      let rewardAddress: string;
      try {
        rewardAddress = dataProvider.getCardanoRewardAddress('registered-with-dust');
      } catch (error) {
        log.warn(error);
        ctx.skip?.(true, (error as Error).message);
        return;
      }

      const dustAddress = await fetchDustAddress(rewardAddress);
      const block = await fetchBlock();
      log.debug(`Subscribing for ${dustAddress} at block ${block.height} (${block.hash})`);

      const events = await collectDustGenerations(indexerWsClient, {
        dustAddress,
        blockHash: block.hash,
        dtimeCutoffHeight: 0,
      });

      expect(events.length).toBeGreaterThan(0);
      assertEventsMatchSchema(events);

      const lastEvent = events[events.length - 1].data!.dustGenerations;
      expect(lastEvent.__typename).toBe('DustGenerationsProgress');
    }, 60_000);
  });

  describe('a subscription pinned to a block hash', () => {
    /**
     * The block-hash snapshot is deterministic: the same block yields the same events.
     *
     * @given a registered dust address and a fixed block hash
     * @when two dustGenerations subscriptions are opened with identical arguments
     * @then both deliver exactly the same event sequence
     *
     * midnight-indexer#1283
     */
    test('should deliver identical events for repeated subscriptions at the same block', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Subscription', 'Dust', 'Generations'] };
      if (!blockHashSurfacePresent) return ctx.skip(true, SURFACE_ABSENT_REASON);

      let rewardAddress: string;
      try {
        rewardAddress = dataProvider.getCardanoRewardAddress('registered-with-dust');
      } catch (error) {
        log.warn(error);
        ctx.skip?.(true, (error as Error).message);
        return;
      }

      const dustAddress = await fetchDustAddress(rewardAddress);
      const block = await fetchBlock();
      const args = { dustAddress, blockHash: block.hash, dtimeCutoffHeight: 0 };

      const firstRun = await collectDustGenerations(indexerWsClient, args);
      const secondRun = await collectDustGenerations(indexerWsClient, args);

      expect(firstRun.length).toBeGreaterThan(0);
      expect(secondRun.length).toBe(firstRun.length);
      expect(secondRun.map((msg) => msg.data!.dustGenerations)).toStrictEqual(
        firstRun.map((msg) => msg.data!.dustGenerations),
      );
    }, 90_000);

    /**
     * The snapshot reflects the queried block's generation tree, not the tip's.
     *
     * Blocks are discovered from queried chain state rather than by arithmetic on
     * the tip height: any tip-relative fraction lands far outside the indexer's
     * snapshot freshness window on a long-running chain. The strong comparison
     * additionally needs the tree to have grown inside that window, which a quiet
     * chain cannot guarantee, so the assertions are tiered — the per-block
     * invariant always runs, the cross-block comparison runs when a growth
     * boundary exists and reports the measured numbers when it does not.
     *
     * @given two blocks inside the freshness window, the older one up to 400 blocks
     *        back — closer when the deployed window is narrower or the chain shorter
     * @when a dustGenerations subscription is opened at each block's hash
     * @then each final progress event reports highestIndex equal to that block's
     *       dustGenerationEndIndex - 1 (on devnet today, endIndex 2452 yields
     *       highestIndex 2451)
     * @and when the tree grew between the two blocks, the pair straddling the newest
     *      growth boundary yields a strictly smaller snapshot before it than after
     *
     * midnight-indexer#1283
     */
    test('should snapshot the generation tree at the queried block rather than the tip', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Subscription', 'Dust', 'Generations'] };
      if (!blockHashSurfacePresent) return ctx.skip(true, SURFACE_ABSENT_REASON);

      // highestIndex is a global tree property — the resolver reports the pinned
      // ledger state's first-free generation index minus one — so it needs no
      // registered wallet and no Cardano fixture.
      const dustAddress = generateDustAddressForNetworkId(env.getNetworkId().toLowerCase());
      const tipBlock = await fetchBlock();

      if (tipBlock.dustGenerationEndIndex === 0) {
        return skipWithReason(
          ctx,
          `generation tree is still empty at the tip (block ${tipBlock.height}, endIndex 0), ` +
            'where the resolver saturates highestIndex to 0 instead of endIndex - 1',
        );
      }

      // Tier A at the tip is always exercisable — offset 0 is inside any window and
      // needs no chain length — so it runs before any block discovery can skip.
      await assertPinnedSnapshot(indexerWsClient, dustAddress, tipBlock);

      // Then step closer to the tip while the server reports the snapshot out of
      // window (a narrower max_snapshot_age) or the tree still empty (closer blocks
      // hold a larger tree). The start is clamped so a short chain never addresses a
      // negative height.
      const startOffset = Math.min(IN_WINDOW_OFFSET, tipBlock.height - 1);
      let earlierBlock: PinnedBlock | undefined;
      let offsetUsed = 0;
      let lastRejection = '';
      for (
        let offset = startOffset;
        offset >= MIN_IN_WINDOW_OFFSET;
        offset = Math.floor(offset / 2)
      ) {
        const candidate = await fetchBlock({ height: tipBlock.height - offset });
        if (candidate.dustGenerationEndIndex === 0) {
          log.warn(`Generation tree still empty at tip-${offset} (block ${candidate.height})`);
          continue;
        }
        const outcome = await assertPinnedSnapshotUnlessStale(
          indexerWsClient,
          dustAddress,
          candidate,
        );
        if (!('rejection' in outcome)) {
          earlierBlock = candidate;
          offsetUsed = offset;
          break;
        }
        lastRejection = outcome.rejection;
        log.warn(
          `Snapshot at tip-${offset} (block ${candidate.height}) refused: ${outcome.rejection}`,
        );
      }

      if (earlierBlock === undefined) {
        const attempted =
          `tip ${tipBlock.height} (endIndex ${tipBlock.dustGenerationEndIndex}), offsets ` +
          `${startOffset} down to ${MIN_IN_WINDOW_OFFSET}`;

        // Being refused even at the smallest offset is not a configuration this
        // deployment can reach: max_snapshot_age defaults to 500, nothing overrides
        // it, and a value under MIN_IN_WINDOW_OFFSET would be absurd. The realistic
        // cause is a regression in the freshness computation, which the negative
        // test cannot catch either — its block stays rejected either way. Fail.
        expect(
          lastRejection,
          `every candidate offset was refused as outside the snapshot freshness window, which ` +
            `no deployable max_snapshot_age explains — ${attempted}`,
        ).toBe('');

        return skipWithReason(ctx, `generation tree empty at every offset tried — ${attempted}`);
      }

      // Tier B — the pinning comparison proper, possible only where the tree grew
      // inside the window.
      const boundaryFound = earlierBlock.dustGenerationEndIndex !== tipBlock.dustGenerationEndIndex;
      const measurements =
        `tip ${tipBlock.height} (endIndex ${tipBlock.dustGenerationEndIndex}), ` +
        `block ${earlierBlock.height} at tip-${offsetUsed} ` +
        `(endIndex ${earlierBlock.dustGenerationEndIndex}), ` +
        `growth boundary found: ${boundaryFound}`;

      if (!boundaryFound) {
        return skipWithReason(
          ctx,
          `generation tree did not grow within the ${offsetUsed}-block in-window budget, ` +
            `so a cross-block comparison would be vacuous — ${measurements}`,
        );
      }

      const { before, after } = await findNewestGrowthBoundary(earlierBlock, tipBlock);
      log.debug(`Tier B comparing blocks ${before.height} and ${after.height} — ${measurements}`);

      // Tier B carries the least margin of anything here: the boundary pair is the
      // oldest thing subscribed to, and the binary search plus these two
      // subscriptions all run after it was selected. A refusal now is the window
      // sliding past the boundary, not a pinning defect.
      const beforeOutcome = await assertPinnedSnapshotUnlessStale(
        indexerWsClient,
        dustAddress,
        before,
      );
      if ('rejection' in beforeOutcome) {
        return skipWithReason(
          ctx,
          `growth boundary aged out of the freshness window before it could be compared: block ` +
            `${before.height} refused (${beforeOutcome.rejection}) — ${measurements}`,
        );
      }
      const afterOutcome = await assertPinnedSnapshotUnlessStale(
        indexerWsClient,
        dustAddress,
        after,
      );
      if ('rejection' in afterOutcome) {
        return skipWithReason(
          ctx,
          `growth boundary aged out of the freshness window before it could be compared: block ` +
            `${after.height} refused (${afterOutcome.rejection}) — ${measurements}`,
        );
      }

      const beforeHighestIndex = beforeOutcome.highestIndex;
      const afterHighestIndex = afterOutcome.highestIndex;
      expect(
        beforeHighestIndex,
        `block ${before.height} should yield a strictly smaller snapshot than block ${after.height}`,
      ).toBeLessThan(afterHighestIndex);
    }, 90_000);
  });

  describe('dtime update delivery relative to the cutoff height', () => {
    /**
     * A zero cutoff replays the wallet's full owned dtime history before the tree events.
     *
     * @given a wallet with spent backing NIGHT UTXOs (registered-with-dust-and-spent)
     * @when a dustGenerations subscription is opened with dtimeCutoffHeight 0
     * @then at least one DustGenerationDtimeUpdateItem is delivered
     * @and every dtime update precedes the first DustGenerationsItem in the stream
     *
     * midnight-indexer#1283 (supersedes the startIndex-based #1167 regression guard)
     */
    test('should replay owned dtime updates before generation items when the cutoff is zero', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Subscription', 'Dust', 'Generations'] };
      if (!blockHashSurfacePresent) return ctx.skip(true, SURFACE_ABSENT_REASON);

      let rewardAddress: string;
      try {
        rewardAddress = dataProvider.getCardanoRewardAddress('registered-with-dust-and-spent');
      } catch (error) {
        log.warn(error);
        ctx.skip?.(true, (error as Error).message);
        return;
      }

      const dustAddress = await fetchDustAddress(rewardAddress);
      const block = await fetchBlock();

      const events = await collectDustGenerations(indexerWsClient, {
        dustAddress,
        blockHash: block.hash,
        dtimeCutoffHeight: 0,
      });

      assertEventsMatchSchema(events);

      const typenames = events.map((msg) => msg.data!.dustGenerations.__typename);
      const dtimeCount = typenames.filter((t) => t === 'DustGenerationDtimeUpdateItem').length;
      log.debug(`Received ${dtimeCount} DustGenerationDtimeUpdateItem event(s)`);
      expect(
        dtimeCount,
        'Expected ≥1 DustGenerationDtimeUpdateItem with dtimeCutoffHeight=0 ' +
          'for a wallet with spent NIGHT UTXOs',
      ).toBeGreaterThanOrEqual(1);

      const firstItemIndex = typenames.indexOf('DustGenerationsItem');
      const lastDtimeIndex = typenames.lastIndexOf('DustGenerationDtimeUpdateItem');
      if (firstItemIndex !== -1) {
        expect(
          lastDtimeIndex,
          'All dtime updates should be issued before the first DustGenerationsItem',
        ).toBeLessThan(firstItemIndex);
      }
    }, 60_000);

    /**
     * A cutoff at the snapshot block suppresses the dtime delta entirely.
     *
     * @given a wallet with spent backing NIGHT UTXOs (registered-with-dust-and-spent)
     * @when a dustGenerations subscription is opened with the dtimeCutoffHeight equal to
     *       the snapshot block's height
     * @then no DustGenerationDtimeUpdateItem is delivered, while the generation snapshot
     *       (items and final progress) still streams and completes
     *
     * midnight-indexer#1283
     */
    test('should deliver no dtime updates when the cutoff equals the snapshot block height', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Subscription', 'Dust', 'Generations'] };
      if (!blockHashSurfacePresent) return ctx.skip(true, SURFACE_ABSENT_REASON);

      let rewardAddress: string;
      try {
        rewardAddress = dataProvider.getCardanoRewardAddress('registered-with-dust-and-spent');
      } catch (error) {
        log.warn(error);
        ctx.skip?.(true, (error as Error).message);
        return;
      }

      const dustAddress = await fetchDustAddress(rewardAddress);
      const block = await fetchBlock();

      const events = await collectDustGenerations(indexerWsClient, {
        dustAddress,
        blockHash: block.hash,
        dtimeCutoffHeight: block.height,
      });

      assertEventsMatchSchema(events);
      expect(eventsOfType(events, 'DustGenerationDtimeUpdateItem')).toHaveLength(0);
      expect(eventsOfType(events, 'DustGenerationsProgress')).toHaveLength(1);
    }, 60_000);
  });

  describe('subscription error handling', () => {
    /**
     * A dust generations subscription with an invalid dust address returns an error.
     *
     * @given an invalid dust address and a valid block hash
     * @when a dustGenerations subscription is opened
     * @then the subscription returns an error
     */
    test('should return an error for an invalid dust address', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Subscription', 'Dust', 'Generations', 'Negative'] };
      if (!blockHashSurfacePresent) return ctx.skip(true, SURFACE_ABSENT_REASON);

      const block = await fetchBlock();
      const errorReceived = await collectDustGenerationsError(indexerWsClient, {
        dustAddress: 'invalid_address',
        blockHash: block.hash,
        dtimeCutoffHeight: 0,
      });

      expect(errorReceived).toBeDefined();
      log.debug(`Received expected error: ${errorReceived}`);
    });

    /**
     * A valid bech32m dust address from another network returns an HRP error.
     *
     * @given valid bech32m dust addresses for all network IDs other than the target one
     *        and a valid block hash
     * @when a dustGenerations subscription is opened for each foreign address
     * @then the indexer returns an error related to an unexpected/wrong HRP prefix
     */
    test('should return an error for a valid address that is meant for another networkid', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Subscription', 'Dust', 'Generations', 'Negative'] };
      if (!blockHashSurfacePresent) return ctx.skip(true, SURFACE_ABSENT_REASON);

      const targetNetworkId = env.getNetworkId().toLowerCase();
      const networkIds = env.getAllEnvironmentNames();
      const block = await fetchBlock();

      for (const networkId of networkIds) {
        if (networkId.toLowerCase() === targetNetworkId) {
          continue;
        }

        const foreignDustAddress = generateDustAddressForNetworkId(networkId);
        log.debug(`Testing foreign dust address for networkId=${networkId}: ${foreignDustAddress}`);

        const result = await collectDustGenerationsError(indexerWsClient, {
          dustAddress: foreignDustAddress,
          blockHash: block.hash,
          dtimeCutoffHeight: 0,
        }).then(
          (error) => ({ error, failure: null as string | null }),
          (failure: Error) => ({ error: null as string | null, failure: failure.message }),
        );

        expect.soft(result.failure, `networkId=${networkId}: ${result.failure}`).toBeNull();
        expect.soft(result.error, `networkId=${networkId} should emit an error`).toBeTruthy();
        if (result.error) {
          expect
            .soft(
              result.error.toLowerCase(),
              `networkId=${networkId} error should mention wrong HRP`,
            )
            .toMatch(/(expected hrp|unexpected hrp|wrong hrp|invalid.*network|network id)/);
        }
      }
    });

    /**
     * A dust address passed in hex format returns a bech32m/HRP error.
     *
     * @given a valid bech32m dust address converted to hex format and a valid block hash
     * @when a dustGenerations subscription is opened using the hex format
     * @then the indexer returns an error indicating the expected bech32m/HRP format
     */
    test('should return an error for a valid dust address passed in hex format', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Subscription', 'Dust', 'Generations', 'Negative'] };
      if (!blockHashSurfacePresent) return ctx.skip(true, SURFACE_ABSENT_REASON);

      const targetNetworkId = env.getNetworkId().toLowerCase();
      const bech32DustAddress = generateDustAddressForNetworkId(targetNetworkId);
      const hexDustAddress = encodeDustAddressAsHex(bech32DustAddress);
      const block = await fetchBlock();

      const errorReceived = await collectDustGenerationsError(indexerWsClient, {
        dustAddress: hexDustAddress,
        blockHash: block.hash,
        dtimeCutoffHeight: 0,
      });

      expect(errorReceived).toBeDefined();
      expect(errorReceived.toLowerCase()).toMatch(
        /(expected hrp|unexpected hrp|wrong hrp|bech32|invalid.*address)/,
      );
    });

    /**
     * A well-formed block hash that matches no indexed block returns an error.
     *
     * @given a valid dust address and a 32-byte hex block hash unknown to the indexer
     * @when a dustGenerations subscription is opened at that block hash
     * @then the indexer returns an "unknown block hash" error
     *
     * midnight-indexer#1283
     */
    test('should return an error for an unknown block hash', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Subscription', 'Dust', 'Generations', 'Negative'] };
      if (!blockHashSurfacePresent) return ctx.skip(true, SURFACE_ABSENT_REASON);

      const targetNetworkId = env.getNetworkId().toLowerCase();
      const dustAddress = generateDustAddressForNetworkId(targetNetworkId);
      const unknownBlockHash = '00'.repeat(32);

      const errorReceived = await collectDustGenerationsError(indexerWsClient, {
        dustAddress,
        blockHash: unknownBlockHash,
        dtimeCutoffHeight: 0,
      });

      expect(errorReceived.toLowerCase()).toMatch(/unknown block hash/);
    });

    /**
     * A block hash that is not valid hex returns an error.
     *
     * @given a valid dust address and a block hash that cannot be hex-decoded
     * @when a dustGenerations subscription is opened at that block hash
     * @then the indexer returns an invalid block hash error
     *
     * midnight-indexer#1283
     */
    test('should return an error for a malformed block hash', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Subscription', 'Dust', 'Generations', 'Negative'] };
      if (!blockHashSurfacePresent) return ctx.skip(true, SURFACE_ABSENT_REASON);

      const targetNetworkId = env.getNetworkId().toLowerCase();
      const dustAddress = generateDustAddressForNetworkId(targetNetworkId);

      const errorReceived = await collectDustGenerationsError(indexerWsClient, {
        dustAddress,
        blockHash: 'not-a-hex-block-hash',
        dtimeCutoffHeight: 0,
      });

      expect(errorReceived.toLowerCase()).toMatch(/(invalid block hash|hex)/);
    });

    /**
     * A snapshot request older than the freshness window is rejected up front.
     *
     * The indexer only keeps recent ledger states loadable, so it refuses a
     * block-pinned snapshot whose block is more than `max_snapshot_age` behind the
     * tip rather than failing mid-stream on garbage-collected state. The window is
     * runtime configuration and is not exposed through the schema, so the block is
     * chosen far enough back to stay outside any plausible value.
     *
     * @given a valid dust address and an indexed block 5000 blocks behind the tip
     * @when a dustGenerations subscription is opened at that block's hash
     * @then the subscription is rejected with the snapshot-freshness message, and not
     *       with either neighbouring rejection (an unknown block hash, or a ledger
     *       state that is no longer available)
     *
     * midnight-indexer#1427
     */
    test('should reject a block hash older than the snapshot freshness window', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Subscription', 'Dust', 'Generations', 'Negative'] };
      if (!blockHashSurfacePresent) return ctx.skip(true, SURFACE_ABSENT_REASON);

      const tipBlock = await fetchBlock();
      const minimumTipHeight = OUT_OF_WINDOW_OFFSET + OUT_OF_WINDOW_MIN_MARGIN;
      if (tipBlock.height <= minimumTipHeight) {
        return skipWithReason(
          ctx,
          `chain is too short to address a block outside the freshness window: tip height ` +
            `${tipBlock.height}, needs more than ${minimumTipHeight}`,
        );
      }

      const staleBlock = await fetchBlock({ height: tipBlock.height - OUT_OF_WINDOW_OFFSET });
      const dustAddress = generateDustAddressForNetworkId(env.getNetworkId().toLowerCase());

      const outcome = await collectDustGenerationsError(indexerWsClient, {
        dustAddress,
        blockHash: staleBlock.hash,
        dtimeCutoffHeight: 0,
      }).then(
        (message) => ({ message, failure: null as string | null }),
        (failure: Error) => ({ message: null as string | null, failure: failure.message }),
      );

      // The guard is not present in every deployed build and cannot be detected by
      // introspection, so a clean completion means the indexer predates it rather
      // than that it regressed. Any other rejection reason is a real failure.
      if (outcome.failure === COMPLETED_WITHOUT_ERROR) {
        return skipWithReason(
          ctx,
          `deployed indexer predates the snapshot freshness guard: a subscription at block ` +
            `${staleBlock.height}, ${OUT_OF_WINDOW_OFFSET} blocks behind tip ` +
            `${tipBlock.height}, streamed a snapshot instead of being rejected`,
        );
      }
      expect(outcome.failure, 'the subscription should have surfaced a server error').toBeNull();

      log.debug(`Received expected freshness rejection: ${outcome.message}`);
      expect(outcome.message).toContain(FRESHNESS_REJECTION);
      expect(outcome.message).not.toMatch(/unknown block hash/);
      expect(outcome.message).not.toMatch(/no longer available/);
    });
  });

  /**
   * Coverage for `transactionHash` on dust generation events
   * (`feat(indexer-api): add transactionHash to event subscription response types`).
   *
   * `transactionHash: HexEncoded!` was added to `DustGenerationsItem` and
   * `DustGenerationDtimeUpdateItem` so wallets can resolve the on-chain
   * transaction from a streamed event via `transactions(offset: { hash: ... })`.
   * The `transactionId` BIGSERIAL is indexer-internal and not portable across
   * indexer instances; the hash is. The schema-level shape (64-hex,
   * non-nullable) is already enforced by the discriminated-union zod schema
   * used by the streaming tests above. This block adds the round-trip check.
   *
   * midnight-indexer#1114
   */
  describe('transactionHash on dust generation events', () => {
    /**
     * @given a registered dust address that emits at least one
     *        `DustGenerationsItem` or `DustGenerationDtimeUpdateItem`
     * @when the first event's `transactionHash` is looked up via
     *       `transactions(offset: { hash: ... })`
     * @then the lookup resolves a single transaction whose `hash` equals the
     *       streamed `transactionHash` — proving the field is the on-chain
     *       identifier wallets can use to fetch the full transaction
     */
    test('first item transactionHash resolves via transactions(offset)', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Subscription', 'Dust', 'Generations', 'Transaction'] };
      if (!blockHashSurfacePresent) return ctx.skip(true, SURFACE_ABSENT_REASON);

      let rewardAddress: string;
      try {
        rewardAddress = dataProvider.getCardanoRewardAddress('registered-with-dust');
      } catch (error) {
        log.warn(error);
        ctx.skip?.(true, (error as Error).message);
        return;
      }

      const dustAddress = await fetchDustAddress(rewardAddress);
      const block = await fetchBlock();

      const events = await collectDustGenerations(indexerWsClient, {
        dustAddress,
        blockHash: block.hash,
        dtimeCutoffHeight: 0,
      });

      const firstItem = events
        .map((msg) => msg.data!.dustGenerations)
        .find(
          (event) =>
            event.__typename === 'DustGenerationsItem' ||
            event.__typename === 'DustGenerationDtimeUpdateItem',
        ) as { transactionId: number; transactionHash: string; __typename: string } | undefined;

      if (firstItem === undefined) {
        ctx.skip?.(
          true,
          'no DustGenerationsItem / DtimeUpdateItem event for this address — round-trip vacuous',
        );
        return;
      }

      log.debug(
        `Round-tripping ${firstItem.__typename}.transactionHash=${firstItem.transactionHash} ` +
          `(transactionId=${firstItem.transactionId})`,
      );

      const txResponse = await indexerHttpClient.getTransactionByOffset({
        hash: firstItem.transactionHash,
      });
      expect(txResponse).toBeSuccess();
      const transactions = txResponse.data!.transactions;
      expect(transactions).toHaveLength(1);
      expect(transactions[0].hash).toBe(firstItem.transactionHash);
    }, 60_000);
  });
});
