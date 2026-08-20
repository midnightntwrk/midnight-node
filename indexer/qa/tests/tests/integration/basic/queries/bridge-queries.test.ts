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

// Integration tests for c2m-bridge GraphQL queries: bridgeEvents, bridgeBalance,
// bridgeDeposits, and claims surfaced as BridgeClaimTransaction (#941).
//
// Test-data reality (2026-07): the bridge is a cross-chain feature — events
// originate from Cardano locks, so they cannot be produced by the indexer's own
// toolkit-driven node-data generation. Of the five BridgeEvent variants only
// BridgeUserTransfer has any on-chain data, and only on devnet. Cases that need
// only the surface (empty results, zero balances) run wherever the surface is
// deployed; cases that need a real UserTransfer run where such data exists and
// ctx.skip otherwise; cases that need the other four variants or non-trivial
// pool state stay test.todo until a data source exists.
//   test.todo → needs bridge data not yet producible in any test environment.
//   test.skip → blocked on a specific in-flight feature (noted inline).
//
// Tracking: https://github.com/midnightntwrk/midnight-indexer/issues/941

import log from '@utils/logging/logger';
import { env } from 'environment/model';
import type { TestContext } from 'vitest';
import '@utils/logging/test-logging-hooks';
import { IndexerHttpClient } from '@utils/indexer/http-client';
import type { BridgeEvent, BridgeUserTransfer } from '@utils/indexer/indexer-types';

const httpClient = new IndexerHttpClient();

// 32-byte all-zeros recipient (HexEncoded) — a valid address with no bridge activity.
const EMPTY_RECIPIENT = '0'.repeat(64);
// 16-byte u128 zero value, as returned by the bridge amount/balance fields.
const ZERO_U128 = '0'.repeat(32);

// The five concrete BridgeEvent union members.
const KNOWN_BRIDGE_TYPENAMES = new Set([
  'BridgeUserTransfer',
  'BridgeReserveTransfer',
  'BridgeInvalidTransfer',
  'BridgeUnapprovedTransfer',
  'BridgeSubminimalFlushTransfer',
]);

// The shared GET_BRIDGE_EVENTS selects id/blockHeight/recipient only on the
// BridgeUserTransfer fragment. Filter/pagination cases need a stable ordering key
// (id) on every variant and the recipient on both recipient-bearing variants, so
// they pass this override to the client.
const BRIDGE_EVENTS_ALL_FIELDS = `
query BridgeEventsAll($RECIPIENT: HexEncoded, $VARIANT: BridgeEventVariant, $BLOCK_HEIGHT_FROM: Int, $BLOCK_HEIGHT_TO: Int, $OFFSET: Int, $LIMIT: Int) {
  bridgeEvents(recipient: $RECIPIENT, variant: $VARIANT, blockHeightFrom: $BLOCK_HEIGHT_FROM, blockHeightTo: $BLOCK_HEIGHT_TO, offset: $OFFSET, limit: $LIMIT) {
    __typename
    ... on BridgeUserTransfer { id blockHeight recipient }
    ... on BridgeReserveTransfer { id blockHeight }
    ... on BridgeInvalidTransfer { id blockHeight }
    ... on BridgeUnapprovedTransfer { id blockHeight recipient }
    ... on BridgeSubminimalFlushTransfer { id blockHeight }
  }
}`;

// A HexEncoded scalar is a hex string; a valid recipient is 32 bytes (64 hex
// chars). These are rejected by the indexer with a hex-decode error.
const MALFORMED_RECIPIENT_ODD_LENGTH = 'abc';
const MALFORMED_RECIPIENT_NON_HEX = 'zz'.repeat(32);

// Reads the `id` every variant carries (optional only on BridgeEventOther).
const bridgeEventId = (e: BridgeEvent): number => Number(e.id);
// Reads `recipient` from the variants that carry one; undefined for the rest.
const bridgeEventRecipient = (e: BridgeEvent): string | undefined =>
  'recipient' in e ? e.recipient : undefined;

// Probed once against the target environment: whether the bridge surface exists,
// a real UserTransfer to assert shape against, and a fully-claimed address (a
// recipient whose deposited > 0 but remaining balance is zero).
let surfacePresent = false;
let sampleUserTransfer: BridgeUserTransfer | null = null;
let fullyClaimedAddress: string | null = null;

// Bridge events cannot be produced on the undeployed environment (no Cardano
// side), so the whole surface is skipped there.
describe.skipIf(env.isUndeployedEnv())('bridge queries', () => {
  beforeAll(async () => {
    const probe = await httpClient.getBridgeEvents({ variant: 'USER_TRANSFER', limit: 1 });
    // A GraphQL error here means the deployed indexer predates the bridge surface
    // (pre-4.4). Leave surfacePresent false so every case skips rather than fails.
    if (probe.errors || !probe.data) {
      log.warn(`Bridge surface not present on ${env.getCurrentEnvironmentName()}; skipping`);
      return;
    }
    surfacePresent = true;

    const first = probe.data.bridgeEvents.find(
      (e): e is BridgeUserTransfer => e.__typename === 'BridgeUserTransfer',
    );
    sampleUserTransfer = first ?? null;

    if (sampleUserTransfer) {
      const balance = await httpClient.getBridgeBalance(sampleUserTransfer.recipient);
      const b = balance.data?.bridgeBalance;
      if (b && b.deposited !== ZERO_U128 && b.balance === ZERO_U128) {
        fullyClaimedAddress = sampleUserTransfer.recipient;
      }
    }
  }, 30_000);

  describe('bridgeEvents', () => {
    /**
     * @given a 32-byte all-zeros recipient address with no bridge activity
     * @when bridgeEvents is queried for that recipient
     * @then the response is successful and bridgeEvents is an empty array
     */
    test('should return an empty list for a recipient address with no bridge activity', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Query', 'Bridge', 'Negative'] };
      if (!surfacePresent) return ctx.skip();

      const response = await httpClient.getBridgeEvents({ recipient: EMPTY_RECIPIENT });

      expect(response).toBeSuccess();
      expect(response.data?.bridgeEvents).toEqual([]);
    });

    /**
     * @given an environment with at least one indexed BridgeUserTransfer
     * @when bridgeEvents(variant: USER_TRANSFER, limit: 1) is queried
     * @then the event exposes id, blockHeight, midnightTxHash, cardanoTxHash, amount, recipient
     * @and the hex fields are non-empty and blockHeight is a positive integer
     */
    test('should return events with the correct shape for BridgeUserTransfer', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Query', 'Bridge', 'UserTransfer'] };
      if (!surfacePresent) return ctx.skip();
      if (!sampleUserTransfer) return ctx.skip(true, 'no BridgeUserTransfer data on this env');

      const event = sampleUserTransfer;
      expect(event.__typename).toBe('BridgeUserTransfer');
      expect(Number.isInteger(event.id)).toBe(true);
      expect(event.blockHeight).toBeGreaterThan(0);
      expect(event.midnightTxHash).toMatch(/^[0-9a-f]+$/);
      expect(event.cardanoTxHash).toMatch(/^[0-9a-f]+$/);
      expect(event.amount).toMatch(/^[0-9a-f]+$/);
      expect(event.recipient).toMatch(/^[0-9a-f]+$/);
    });

    /**
     * @given an environment whose chain has bridge events for a known recipient
     * @when bridgeEvents(recipient: <knownAddress>) is queried
     * @then every returned event echoes recipient equal to <knownAddress>
     */
    test('should return only events for the specified recipient address', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Query', 'Bridge', 'Filter'] };
      if (!surfacePresent) return ctx.skip();
      if (!sampleUserTransfer) return ctx.skip(true, 'no BridgeUserTransfer data on this env');

      const recipient = sampleUserTransfer.recipient;
      const response = await httpClient.getBridgeEvents({ recipient }, BRIDGE_EVENTS_ALL_FIELDS);

      expect(response).toBeSuccess();
      const events = response.data!.bridgeEvents;
      // The known recipient must have at least its UserTransfer.
      expect(events.length).toBeGreaterThan(0);
      // Only recipient-bearing variants can match a recipient filter, and each
      // must echo exactly the filtered recipient.
      for (const event of events) {
        expect(bridgeEventRecipient(event)).toBe(recipient);
      }
    });

    /**
     * @given a chain whose bridge events span several variants
     * @when bridgeEvents is queried with no filters
     * @then every returned __typename is one of the five known bridge variants
     * @and at least one variant is present (the env has the surface + Cardano data)
     *
     * NOTE: RESERVE_TRANSFER is not asserted present — it requires reserve-contract
     * activity the c2m-bridge flood does not drive, so it is commonly absent.
     */
    test('should return events of the known variant types present on chain', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Query', 'Bridge', 'Variants'] };
      if (!surfacePresent) return ctx.skip();

      const response = await httpClient.getBridgeEvents({ limit: 100 }, BRIDGE_EVENTS_ALL_FIELDS);

      expect(response).toBeSuccess();
      const seen = new Set(response.data!.bridgeEvents.map((e) => e.__typename));
      expect(seen.size).toBeGreaterThan(0);
      for (const typename of seen) {
        expect(KNOWN_BRIDGE_TYPENAMES.has(typename)).toBe(true);
      }
    });

    /**
     * @given a chain with at least 3 indexed bridge events (ordered by ascending id)
     * @when bridgeEvents(limit: 2, offset: 0) and (limit: 2, offset: 1) are queried
     * @then the two pages overlap by exactly 1 event and ids are ascending
     */
    test('should paginate results with offset and limit', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Query', 'Bridge', 'Pagination'] };
      if (!surfacePresent) return ctx.skip();

      const all = await httpClient.getBridgeEvents({ limit: 100 }, BRIDGE_EVENTS_ALL_FIELDS);
      expect(all).toBeSuccess();
      const total = all.data!.bridgeEvents.length;
      if (total < 3) return ctx.skip(true, `need >= 3 bridge events to paginate, found ${total}`);

      const page0 = await httpClient.getBridgeEvents(
        { limit: 2, offset: 0 },
        BRIDGE_EVENTS_ALL_FIELDS,
      );
      const page1 = await httpClient.getBridgeEvents(
        { limit: 2, offset: 1 },
        BRIDGE_EVENTS_ALL_FIELDS,
      );
      expect(page0).toBeSuccess();
      expect(page1).toBeSuccess();

      const p0 = page0.data!.bridgeEvents;
      const p1 = page1.data!.bridgeEvents;
      expect(p0).toHaveLength(2);
      expect(p1).toHaveLength(2);

      // A one-row offset shift makes page0's second row equal page1's first row.
      expect(bridgeEventId(p0[1])).toBe(bridgeEventId(p1[0]));
      // Default order is ascending id within each page.
      expect(bridgeEventId(p0[0])).toBeLessThan(bridgeEventId(p0[1]));
      expect(bridgeEventId(p1[0])).toBeLessThan(bridgeEventId(p1[1]));
    });

    /**
     * @given a chain containing indexed UserTransfer events
     * @when bridgeEvents(variant: USER_TRANSFER) is queried
     * @then every event has __typename BridgeUserTransfer
     */
    test('should return only UserTransfer events when filtered by variant USER_TRANSFER', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Query', 'Bridge', 'Filter', 'UserTransfer'] };
      if (!surfacePresent) return ctx.skip();
      if (!sampleUserTransfer) return ctx.skip(true, 'no BridgeUserTransfer data on this env');

      const response = await httpClient.getBridgeEvents({ variant: 'USER_TRANSFER', limit: 100 });

      expect(response).toBeSuccess();
      const events = response.data!.bridgeEvents;
      expect(events.length).toBeGreaterThan(0);
      for (const event of events) {
        expect(event.__typename).toBe('BridgeUserTransfer');
      }
    });

    /**
     * @given the bridge surface
     * @when bridgeEvents(recipient:) is given a malformed HexEncoded value
     * @then the indexer rejects it with a GraphQL error rather than returning data
     */
    test('should reject a malformed recipient with a GraphQL error', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Query', 'Bridge', 'Negative', 'Validation'] };
      if (!surfacePresent) return ctx.skip();

      for (const bad of [MALFORMED_RECIPIENT_ODD_LENGTH, MALFORMED_RECIPIENT_NON_HEX]) {
        const response = await httpClient.getBridgeEvents({ recipient: bad });
        expect(response).toBeError();
      }
    });
  });

  describe('claims via unshieldedTransactions', () => {
    // A claim is surfaced as a BridgeClaimTransaction (a ClaimRewards transaction
    // with ClaimKind CardanoBridge) via the existing unshieldedTransactions query,
    // not a bridge-specific query.
    /**
     * @given a chain containing a CardanoBridge claim for a known recipient
     * @when unshieldedTransactions is queried for that recipient
     * @then the matched transaction is a BridgeClaimTransaction with the bridged recipient and amount
     */
    test.todo(
      'should surface a BridgeClaimTransaction (recipient, amount) via unshieldedTransactions for a claim recipient',
    );
  });

  describe('bridgeBalance', () => {
    /**
     * @given a 32-byte all-zeros address with no bridge activity
     * @when bridgeBalance(address: <zeros>) is queried
     * @then deposited, claimed and balance are all the zero-value hex string
     */
    test('should return zero balance for an address with no bridge activity', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Query', 'Bridge', 'Balance', 'Negative'] };
      if (!surfacePresent) return ctx.skip();

      const response = await httpClient.getBridgeBalance(EMPTY_RECIPIENT);

      expect(response).toBeSuccess();
      expect(response.data?.bridgeBalance).toEqual({
        deposited: ZERO_U128,
        claimed: ZERO_U128,
        balance: ZERO_U128,
      });
    });

    /**
     * @given an address with a UserTransfer deposit that has been fully claimed
     * @when bridgeBalance(address: <address>) is queried
     * @then deposited and claimed are non-zero and balance is the zero-value hex string
     *
     * The remaining-claimable balance is read from the ledger's bridge_receiving map.
     */
    test('should return zero balance once an address has fully claimed', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Query', 'Bridge', 'Balance'] };
      if (!surfacePresent) return ctx.skip();
      if (!fullyClaimedAddress)
        return ctx.skip(true, 'no fully-claimed bridge address on this env');

      const response = await httpClient.getBridgeBalance(fullyClaimedAddress);

      expect(response).toBeSuccess();
      const b = response.data!.bridgeBalance;
      expect(b.deposited).not.toBe(ZERO_U128);
      expect(b.claimed).not.toBe(ZERO_U128);
      expect(b.balance).toBe(ZERO_U128);
    });

    /**
     * @given an address with UserTransfer events
     * @when bridgeBalance(address: <knownAddress>) is queried
     * @then deposited equals the sum of UserTransfer.amount values for that address
     *
     * Amounts are hex u128 strings of differing widths (event vs balance field),
     * so they are compared as BigInt values rather than string-equal.
     */
    test('should set deposited to the sum of UserTransfer amounts for the address', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Query', 'Bridge', 'Balance'] };
      if (!surfacePresent) return ctx.skip();
      if (!sampleUserTransfer) return ctx.skip(true, 'no BridgeUserTransfer data on this env');

      const recipient = sampleUserTransfer.recipient;
      const events = await httpClient.getBridgeEvents({
        recipient,
        variant: 'USER_TRANSFER',
        limit: 100,
      });
      expect(events).toBeSuccess();
      const userTransfers = events.data!.bridgeEvents as BridgeUserTransfer[];
      expect(userTransfers.length).toBeGreaterThan(0);
      const expectedDeposited = userTransfers.reduce((acc, e) => acc + BigInt(`0x${e.amount}`), 0n);

      const balance = await httpClient.getBridgeBalance(recipient);
      expect(balance).toBeSuccess();
      expect(BigInt(`0x${balance.data!.bridgeBalance.deposited}`)).toBe(expectedDeposited);
    });

    /**
     * @given an address with a UserTransfer and a subsequent partial claim
     * @when bridgeBalance(address: <address>) is queried
     * @then balance is a non-zero hex string reflecting the remaining-claimable amount
     */
    test.todo('should reflect a non-zero remaining-claimable balance after a partial claim');
  });

  describe('bridgeDeposits', () => {
    /**
     * @given a 32-byte all-zeros address with no deposits
     * @when bridgeDeposits(recipient: <zeros>) is queried
     * @then the response is an empty array
     */
    test('should return an empty list for an address with no deposits', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Query', 'Bridge', 'Deposits', 'Negative'] };
      if (!surfacePresent) return ctx.skip();

      const response = await httpClient.getBridgeDeposits(EMPTY_RECIPIENT);

      expect(response).toBeSuccess();
      expect(response.data?.bridgeDeposits).toEqual([]);
    });

    /**
     * @given a chain with UserTransfer (and possibly UnapprovedTransfer) for an address
     * @when bridgeDeposits(recipient: <address>) is queried without includeUnapproved
     * @then only BridgeUserTransfer events are returned (no BridgeUnapprovedTransfer)
     */
    test('should return only UserTransfer events by default (excludes UnapprovedTransfer)', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Query', 'Bridge', 'Deposits'] };
      if (!surfacePresent) return ctx.skip();
      if (!sampleUserTransfer) return ctx.skip(true, 'no BridgeUserTransfer data on this env');

      const response = await httpClient.getBridgeDeposits(sampleUserTransfer.recipient);

      expect(response).toBeSuccess();
      const deposits = response.data!.bridgeDeposits;
      expect(deposits.length).toBeGreaterThan(0);
      for (const event of deposits) {
        expect(event.__typename).toBe('BridgeUserTransfer');
      }
    });

    /**
     * @given an address that has an UnapprovedTransfer deposit
     * @when bridgeDeposits(recipient: <address>, includeUnapproved: true) is queried
     * @then the result is a superset of the default (approved) deposits
     * @and it contains at least one BridgeUnapprovedTransfer
     *
     * Was blocked (#940) on the node's approval governance; that logic ships in
     * node >= 2.0.0-rc.3, so UnapprovedTransfer is now produced on a Cardano-backed
     * stack. Skips gracefully where the recipient has no unapproved deposit.
     */
    test('should include UnapprovedTransfer events when includeUnapproved=true', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Query', 'Bridge', 'Deposits'] };
      if (!surfacePresent) return ctx.skip();
      if (!sampleUserTransfer) return ctx.skip(true, 'no BridgeUserTransfer data on this env');

      const recipient = sampleUserTransfer.recipient;
      const withUnapproved = await httpClient.getBridgeDeposits(recipient, {
        includeUnapproved: true,
      });
      expect(withUnapproved).toBeSuccess();
      const events = withUnapproved.data!.bridgeDeposits;

      const hasUnapproved = events.some((e) => e.__typename === 'BridgeUnapprovedTransfer');
      if (!hasUnapproved) {
        return ctx.skip(true, 'no UnapprovedTransfer deposit for this recipient on this env');
      }
      // includeUnapproved must not drop the approved deposits — it is a superset.
      const defaultDeposits = await httpClient.getBridgeDeposits(recipient);
      expect(defaultDeposits).toBeSuccess();
      expect(events.length).toBeGreaterThanOrEqual(defaultDeposits.data!.bridgeDeposits.length);
    });
  });
});
