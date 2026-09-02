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

// Smoke-level schema presence checks for the c2m-bridge GraphQL surface.
//
// These tests verify that every bridge type, query, and subscription declared
// in #941 and #942 actually appears in the deployed schema. They run via
// introspection so they cost only one HTTP round-trip each.
//
// The bridge feature is @beta and not present on every environment. A beforeAll
// introspects whether the surface exists and the suite skips itself where it is
// absent, so a normal smoke run on a non-bridge env (e.g. undeployed) is green
// rather than red. Run against an env that has it deployed (e.g.
// TARGET_ENV=devnet) to exercise the assertions.
//
// Tracking: https://github.com/midnightntwrk/midnight-indexer/issues/941
//           https://github.com/midnightntwrk/midnight-indexer/issues/942

import log from '@utils/logging/logger';
import '@utils/logging/test-logging-hooks';
import type { TestContext } from 'vitest';
import { IndexerHttpClient } from '@utils/indexer/http-client';

const INTROSPECT_TYPE = (name: string) => `
  query {
    __type(name: "${name}") {
      name
      kind
      fields { name }
      enumValues { name }
      possibleTypes { name }
      interfaces { name }
    }
  }
`;

const INTROSPECT_ROOT_FIELDS = (rootType: 'Query' | 'Subscription') => `
  query {
    __type(name: "${rootType}") {
      fields { name }
    }
  }
`;

interface IntrospectionTypeResult {
  __type: {
    name: string;
    kind: string;
    fields: { name: string }[] | null;
    enumValues: { name: string }[] | null;
    possibleTypes: { name: string }[] | null;
    interfaces: { name: string }[] | null;
  } | null;
}

const httpClient = new IndexerHttpClient();

// Raw introspection over HTTP, mirroring tests/smoke/graphql-healthchecks.test.ts.
function rawRequest<T>(query: string): Promise<{ data: T | null }> {
  return (
    httpClient as unknown as {
      client: { rawRequest: (q: string) => Promise<{ data: T | null }> };
    }
  ).client.rawRequest(query);
}

async function introspectType(name: string) {
  const response = await rawRequest<IntrospectionTypeResult>(INTROSPECT_TYPE(name));
  return response.data?.__type ?? null;
}

async function rootFieldNames(rootType: 'Query' | 'Subscription') {
  const response = await rawRequest<IntrospectionTypeResult>(INTROSPECT_ROOT_FIELDS(rootType));
  return (response.data?.__type?.fields ?? []).map((f) => f.name);
}

describe('bridge GraphQL schema', () => {
  // The bridge surface is feature-gated and absent on some environments. Probe
  // once and skip every test below where it is not deployed.
  let bridgeAvailable = false;
  beforeAll(async () => {
    bridgeAvailable = (await introspectType('BridgeEvent')) !== null;
    if (!bridgeAvailable) {
      log.warn(
        'Bridge GraphQL surface not present on this environment — skipping bridge schema smoke checks.',
      );
    }
  });

  // #941 — GraphQL types for bridge events
  describe('types', () => {
    /**
     * @given an indexer deployment with the bridge surface
     * @when the BridgeEvent type is introspected
     * @then it is an INTERFACE exposing the flat fields id, blockHeight and midnightTxHash
     * @and it does not expose a nested indexedAt field
     */
    test('should expose BridgeEvent interface with flat blockHeight', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Smoke', 'Bridge', 'Schema'] };
      if (!bridgeAvailable) {
        ctx.skip();
        return;
      }
      const type = await introspectType('BridgeEvent');
      expect(type).not.toBeNull();
      expect(type?.kind).toBe('INTERFACE');
      const fields = (type?.fields ?? []).map((f) => f.name);
      expect(fields).toEqual(expect.arrayContaining(['id', 'blockHeight', 'midnightTxHash']));
      expect(fields).not.toContain('indexedAt');
    });

    /**
     * @given an indexer deployment with the bridge surface
     * @when each concrete bridge event type is introspected
     * @then every variant exists and declares BridgeEvent as an implemented interface
     */
    test('should expose the five concrete BridgeEvent variants', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Smoke', 'Bridge', 'Schema'] };
      if (!bridgeAvailable) {
        ctx.skip();
        return;
      }
      const variants = [
        'BridgeUserTransfer',
        'BridgeReserveTransfer',
        'BridgeInvalidTransfer',
        'BridgeUnapprovedTransfer',
        'BridgeSubminimalFlushTransfer',
      ];
      for (const name of variants) {
        const type = await introspectType(name);
        expect(type, `${name} should exist`).not.toBeNull();
        const interfaces = (type?.interfaces ?? []).map((i) => i.name);
        expect(interfaces, `${name} should implement BridgeEvent`).toContain('BridgeEvent');
      }
    });

    /**
     * @given an indexer deployment with the bridge surface
     * @when the BridgeSubminimalFlushTransfer type is introspected
     * @then it exposes amount and count and carries no Cardano tx hash
     */
    test('should expose BridgeSubminimalFlushTransfer without a cardanoTxHash', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Smoke', 'Bridge', 'Schema'] };
      if (!bridgeAvailable) {
        ctx.skip();
        return;
      }
      const type = await introspectType('BridgeSubminimalFlushTransfer');
      expect(type).not.toBeNull();
      const fields = (type?.fields ?? []).map((f) => f.name);
      expect(fields).toEqual(expect.arrayContaining(['amount', 'count']));
      expect(fields).not.toContain('cardanoTxHash');
    });

    /**
     * @given an indexer deployment with the bridge surface
     * @when the BridgeClaimTransaction type is introspected
     * @then it exists and implements the Transaction interface
     */
    test('should expose BridgeClaimTransaction implementing Transaction', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Smoke', 'Bridge', 'Schema'] };
      if (!bridgeAvailable) {
        ctx.skip();
        return;
      }
      const type = await introspectType('BridgeClaimTransaction');
      expect(type).not.toBeNull();
      const interfaces = (type?.interfaces ?? []).map((i) => i.name);
      expect(interfaces).toContain('Transaction');
    });

    /**
     * @given an indexer deployment with the bridge surface
     * @when the BridgeBalance type is introspected
     * @then it exposes deposited, claimed and balance and no per-type address field
     */
    test('should expose BridgeBalance with deposited, claimed, balance', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Smoke', 'Bridge', 'Schema'] };
      if (!bridgeAvailable) {
        ctx.skip();
        return;
      }
      const type = await introspectType('BridgeBalance');
      expect(type).not.toBeNull();
      const fields = (type?.fields ?? []).map((f) => f.name);
      expect(fields).toEqual(expect.arrayContaining(['deposited', 'claimed', 'balance']));
      expect(fields).not.toContain('address');
    });

    /**
     * @given an indexer deployment with the bridge surface
     * @when the BridgeEventVariant enum is introspected
     * @then it exposes exactly the five pallet event discriminators
     */
    test('should expose BridgeEventVariant enum with five values', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Smoke', 'Bridge', 'Schema'] };
      if (!bridgeAvailable) {
        ctx.skip();
        return;
      }
      const type = await introspectType('BridgeEventVariant');
      expect(type).not.toBeNull();
      expect(type?.kind).toBe('ENUM');
      const values = (type?.enumValues ?? []).map((v) => v.name).sort();
      expect(values).toEqual(
        [
          'INVALID_TRANSFER',
          'RESERVE_TRANSFER',
          'SUBMINIMAL_FLUSH_TRANSFER',
          'UNAPPROVED_TRANSFER',
          'USER_TRANSFER',
        ].sort(),
      );
    });
  });

  // #941 — bridge query root fields
  describe('queries', () => {
    /**
     * @given an indexer deployment with the bridge surface
     * @when the Query root type is introspected
     * @then the bridgeEvents, bridgeBalance and bridgeDeposits fields are present
     */
    test('should expose the bridge query fields on Query', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Smoke', 'Bridge', 'Schema'] };
      if (!bridgeAvailable) {
        ctx.skip();
        return;
      }
      const fields = await rootFieldNames('Query');
      expect(fields).toEqual(
        expect.arrayContaining(['bridgeEvents', 'bridgeBalance', 'bridgeDeposits']),
      );
    });

    // Note: the bridge pool query surface (bridgeReserveInflows, bridgeTreasuryInflows,
    // bridgePoolSummary) is owned by the pool-observability work (#944) and is asserted
    // in that branch's tests, not here.
  });

  // #942 — bridge subscription root fields
  describe('subscriptions', () => {
    /**
     * @given an indexer deployment with the bridge surface
     * @when the Subscription root type is introspected
     * @then the bridgeEvents and bridgeBalance subscription fields are present
     */
    test('should expose the bridge subscription fields on Subscription', async (ctx: TestContext) => {
      ctx.task!.meta.custom = { labels: ['Smoke', 'Bridge', 'Schema'] };
      if (!bridgeAvailable) {
        ctx.skip();
        return;
      }
      const fields = await rootFieldNames('Subscription');
      expect(fields).toEqual(expect.arrayContaining(['bridgeEvents', 'bridgeBalance']));
    });

    // Note: the bridgePoolUpdates subscription is owned by the pool-observability
    // work (#944) and is asserted in that branch's tests, not here.
  });
});
