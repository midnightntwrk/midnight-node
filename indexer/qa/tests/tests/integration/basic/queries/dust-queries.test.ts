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

import { bech32 } from 'bech32';
import { Buffer } from 'node:buffer';
import log from '@utils/logging/logger';
import { env } from 'environment/model';
import type { TestContext } from 'vitest';
import '@utils/logging/test-logging-hooks';
import dataProvider from '@utils/testdata-provider';
import { IndexerHttpClient } from '@utils/indexer/http-client';
import { ToolkitWrapper } from '@utils/toolkit/toolkit-wrapper';
import { DustGenerationStatusSchema } from '@utils/indexer/graphql/schema';
import type { DustGenerationStatusResponse } from '@utils/indexer/indexer-types';

// Ledger parameters
const GENERATION_DECAY_RATE = 8267;
const MAX_SPECK_PER_STAR = 5n * 10n ** 9n; // Same as saying 5 DUST per NIGHT

// Unit conversion constants
const STAR_PER_NIGHT = 1_000_000n; // 1 NIGHT = 10^6 STAR
const SPECK_PER_DUST = 10n ** 15n; // 1 DUST = 10^15 SPECK
// Minimum expected maxCapacity for 1 NIGHT = 5 DUST = 5 * 10^15 SPECK
const MIN_MAX_CAPACITY_FOR_ONE_NIGHT = 5n * SPECK_PER_DUST;

const indexerHttpClient = new IndexerHttpClient();

type AcceptedRewardAddressHrpPrefix = 'stake' | 'stake_test';

function generateRewardAddress(
  byteValue: number,
  hrpPrefix: AcceptedRewardAddressHrpPrefix | undefined = undefined,
): string {
  // This is to allow overriding for negative tests
  if (hrpPrefix === undefined) {
    hrpPrefix = env.getCurrentEnvironmentName() === 'mainnet' ? 'stake' : 'stake_test';
  }
  const payload = Buffer.alloc(29, byteValue);
  return bech32.encode(hrpPrefix, bech32.toWords(payload));
}

const TOOLKIT_STARTUP_TIMEOUT = 60_000;

// Dust generation registrations require a Cardano-side mapping which has no
// counterpart in the `undeployed` environment. Skip the whole surface there;
// re-enable once #1152 lands local Cardano test-data provisioning.
describe.skipIf(env.isUndeployedEnv())('dust generation status queries', () => {
  let toolkit: ToolkitWrapper;

  beforeAll(async () => {
    toolkit = new ToolkitWrapper({});
    await toolkit.start();
  }, TOOLKIT_STARTUP_TIMEOUT);

  afterAll(async () => {
    await toolkit.stop();
  });

  describe('a dust generation status query with a valid Cardano reward address', () => {
    /**
     * A dust generation status query for a valid Cardano reward address should repond with the expected schema
     *
     * NOTE: here we are not really interested in the status per se, but rather that we get an object back
     * that describes the dust generation status of the requested reward address
     *
     * NOTE2: the generation status is an array so if we request the status for N keys, we will get an array
     * of N statuses.. in this case N=1
     *
     * @given we have a valid Cardano reward address
     * @when we send a dust generation status query with that key
     * @then Indexer should respond with a dust generation status response according to the requested schema
     */
    test('should respond with a dust generation status response according to the requested schema', async (ctx: TestContext) => {
      ctx.task!.meta.custom = {
        labels: ['Query', 'Dust', 'Tokenomics', 'cNgD', 'SchemaValidation'],
        testKey: 'PM-18407',
      };

      const rewardAddress = generateRewardAddress(0);

      const response: DustGenerationStatusResponse =
        await indexerHttpClient.getDustGenerationStatus([rewardAddress]);

      log.debug('Checking if we actually received a dust generation status');
      expect(response).toBeSuccess();
      const dustGenerationStatus = response.data?.dustGenerationStatus;
      expect(dustGenerationStatus).toBeDefined();
      expect(Array.isArray(dustGenerationStatus)).toBe(true);
      expect(dustGenerationStatus).toHaveLength(1);

      log.debug('Validating dust generation status schema');
      const status = DustGenerationStatusSchema.safeParse(response.data?.dustGenerationStatus[0]);
      expect(
        status.success,
        `DUST generation status schema validation failed ${JSON.stringify(status.error, null, 2)}`,
      ).toBe(true);
    });

    /**
     * A dust generation status query for a registered Cardano reward address should give
     * the registered status for the address
     *
     * @given we have a registered Cardano reward address
     * @when we query the dust generation status for that address
     * @then the address should be marked as registered and all generation values should be non-zero
     */
    test('should report registered status for a registered Cardano reward address', async (ctx: TestContext) => {
      ctx.task!.meta.custom = {
        labels: ['Query', 'Dust', 'Tokenomics', 'cNgD'],
        testKey: 'PM-18408',
      };

      let registeredRewardAddress: string;
      try {
        registeredRewardAddress = dataProvider.getCardanoRewardAddress('registered-with-dust');
      } catch (error) {
        log.warn(error);
        ctx.skip?.(true, (error as Error).message);
      }

      // Query registered key
      const registeredResponse: DustGenerationStatusResponse =
        await indexerHttpClient.getDustGenerationStatus([registeredRewardAddress!]);

      expect(registeredResponse).toBeSuccess();
      const dustGenerationStatus = registeredResponse.data?.dustGenerationStatus;
      expect(dustGenerationStatus).toBeDefined();
      expect(Array.isArray(dustGenerationStatus)).toBe(true);
      expect(dustGenerationStatus?.length).toBe(1);
      const registeredStatus = dustGenerationStatus![0];
      expect(registeredStatus?.registered).toBe(true);
      expect(registeredStatus?.dustAddress).toBeDefined();
      // nightBalance is in STAR (1 NIGHT = 10^6 STAR), so any real cNIGHT holder must have >= 1_000_000
      expect(BigInt(registeredStatus?.nightBalance)).toBeGreaterThanOrEqual(STAR_PER_NIGHT);
      expect(BigInt(registeredStatus?.generationRate)).toBeGreaterThan(0n);
      // currentCapacity is in SPECK — for any address generating for more than a few minutes, expect at least 1 DUST
      expect(BigInt(registeredStatus?.currentCapacity)).toBeGreaterThanOrEqual(SPECK_PER_DUST);
      // maxCapacity is in SPECK (1 DUST = 10^15 SPECK), for 1 NIGHT the cap is 5 DUST = 5 * 10^15 SPECK
      expect(BigInt(registeredStatus?.maxCapacity)).toBeGreaterThanOrEqual(
        MIN_MAX_CAPACITY_FOR_ONE_NIGHT,
      );
      expect(registeredStatus?.utxoTxHash).not.toBeNull();
      expect(registeredStatus?.utxoTxHash).toMatch(/^[a-f0-9]{64}$/);
      expect(registeredStatus?.utxoOutputIndex).not.toBeNull();
      expect(registeredStatus?.utxoOutputIndex).toBeGreaterThanOrEqual(0);
    });

    /**
     * A dust generation status query for a registered Cardano reward address should give
     * the DUST destination address for the expected network
     *
     * @given we have a registered Cardano reward address
     * @when we query the dust generation status for that address
     * @then the DUST destination address for the expected network should be returned
     */
    test('should give the DUST destination address for the expected network when Cardano reward address is registered', async (ctx: TestContext) => {
      ctx.task!.meta.custom = {
        labels: ['Query', 'Dust', 'Tokenomics', 'cNgD'],
        testKey: 'PM-17341',
      };

      let registeredRewardAddress: string;
      try {
        registeredRewardAddress = dataProvider.getCardanoRewardAddress('registered-with-dust');
      } catch (error) {
        log.warn(error);
        ctx.skip?.(true, (error as Error).message);
      }

      // Query registered key
      const registeredResponse: DustGenerationStatusResponse =
        await indexerHttpClient.getDustGenerationStatus([registeredRewardAddress!]);

      expect(registeredResponse).toBeSuccess();
      const dustGenerationStatus = registeredResponse.data?.dustGenerationStatus[0];
      expect(dustGenerationStatus?.dustAddress).toBeDefined();
      expect(dustGenerationStatus?.dustAddress).not.toBeNull();

      // The DUST destination address should have hrp prefix for the target network
      const dustAddressHrpPrefix = 'mn_dust_' + env.getNetworkId();
      expect(dustGenerationStatus?.dustAddress).toMatch(new RegExp(`^${dustAddressHrpPrefix}`));
    });

    /**
     * A dust generation status query for a non-registered Cardano reward address should repond with
     * that address marked as not registered and all generation values set to zero
     *
     * @given we have a non-registered Cardano reward address
     * @when we query the dust generation status for that address
     * @then the address should be marked as not registered and all generation values should be zero
     */
    test('should correctly indicate registration status for a non-registered key', async (ctx: TestContext) => {
      ctx.task!.meta.custom = {
        labels: ['Query', 'Dust', 'Tokenomics', 'cNgD'],
        testKey: 'PM-18409',
      };

      let nonRegisteredRewardAddress: string;
      try {
        nonRegisteredRewardAddress = dataProvider.getCardanoRewardAddress('non-registered');
      } catch (error) {
        log.warn(error);
        ctx.skip?.(true, (error as Error).message);
      }

      // Query non-registered key
      const nonRegisteredResponse: DustGenerationStatusResponse =
        await indexerHttpClient.getDustGenerationStatus([nonRegisteredRewardAddress!]);

      expect(nonRegisteredResponse).toBeSuccess();
      const registeredStatus = nonRegisteredResponse.data?.dustGenerationStatus[0];
      expect(registeredStatus?.registered).toBe(false);
      expect(registeredStatus?.dustAddress).toBeNull();
      expect(registeredStatus?.nightBalance).toBe('0');
      expect(registeredStatus?.generationRate).toBe('0');
      expect(registeredStatus?.currentCapacity).toBe('0');
      expect(registeredStatus?.maxCapacity).toBe('0');
      expect(registeredStatus?.utxoTxHash).toBeNull();
      expect(registeredStatus?.utxoOutputIndex).toBeNull();
    });

    /**
     * A dust generation status query correctly indicates zero DUST generation for a registered Cardano reward address
     * without cNIGHT balance
     *
     * This test verifies that when a Cardano reward address is registered for DUST production but has no cNIGHT balance,
     * the status correctly reflects that no DUST can be generated due to the lack of cNIGHT.
     *
     * @given we have a Cardano reward address that is registered for DUST production but has no cNIGHT balance
     * @when we query the DUST generation status
     * @then the status should indicate registered=true with a dustAddress, but all generation values should be zero
     *       because there is no cNIGHT to generate DUST from
     */
    test('should indicate zero generation for registered address without cNIGHT balance', async (ctx: TestContext) => {
      ctx.task!.meta.custom = {
        labels: ['Query', 'Dust', 'Tokenomics', 'cNgD'],
        testKey: 'PM-18413',
      };

      let registeredWithoutDustAddress: string;
      try {
        registeredWithoutDustAddress =
          dataProvider.getCardanoRewardAddress('registered-without-dust');
      } catch (error) {
        log.warn(error);
        ctx.skip?.(true, (error as Error).message);
      }

      const response: DustGenerationStatusResponse =
        await indexerHttpClient.getDustGenerationStatus([registeredWithoutDustAddress!]);

      expect(response).toBeSuccess();
      const dustGenerationStatus = response.data?.dustGenerationStatus;
      expect(dustGenerationStatus).toBeDefined();
      expect(Array.isArray(dustGenerationStatus)).toBe(true);
      expect(dustGenerationStatus?.length).toBe(1);

      const status = dustGenerationStatus![0];
      // Address is registered for DUST production
      expect(status?.registered).toBe(true);
      expect(status?.dustAddress).toBeDefined();
      expect(status?.dustAddress).not.toBeNull();

      // But has no cNIGHT balance, so no dust can be generated
      expect(status?.nightBalance).toBe('0');
      expect(status?.generationRate).toBe('0');
      expect(status?.currentCapacity).toBe('0');
      expect(status?.maxCapacity).toBe('0');
    });

    /**
     * A dust generation status query correctly indicates generation rate and capacity for a registered address with
     * positive cNIGHT balance
     *
     * This test verifies that when a Cardano reward address is registered for DUST production and has positive cNIGHT balance,
     * the generation rate equals cNIGHT balance * 8267 and the current capacity is calculated correctly.
     *
     * @given we have a Cardano reward address that is registered for DUST production with positive cNIGHT balance
     * @when we query the DUST generation status
     * @then the status should indicate registered=true, nightBalance > 0, generationRate = nightBalance * 8267,
     *       currentCapacity = between 0 and maxCapacity, and finally maxCapacity = nightBalance * 5 * 10^9
     */
    test('should report the correct value of max capacity for registered address with cNIGHT', async (ctx: TestContext) => {
      ctx.task!.meta.custom = {
        labels: ['Query', 'Dust', 'Tokenomics', 'cNgD'],
        testKey: 'PM-18414',
      };

      let registeredWithDustAddress: string;
      try {
        registeredWithDustAddress = dataProvider.getCardanoRewardAddress('registered-with-dust');
      } catch (error) {
        log.warn(error);
        ctx.skip?.(true, (error as Error).message);
      }

      const response: DustGenerationStatusResponse =
        await indexerHttpClient.getDustGenerationStatus([registeredWithDustAddress!]);

      expect(response).toBeSuccess();
      const dustGenerationStatus = response.data?.dustGenerationStatus;
      expect(dustGenerationStatus).toBeDefined();
      expect(Array.isArray(dustGenerationStatus)).toBe(true);
      expect(dustGenerationStatus?.length).toBe(1);

      const status = dustGenerationStatus![0];
      // Address is registered for DUST production
      expect(status?.registered).toBe(true);
      expect(status?.dustAddress).not.toBeNull();
      expect(status?.dustAddress).toBeDefined();

      // Has cNIGHT balance — nightBalance is in STAR (1 NIGHT = 10^6 STAR)
      // Any registered address with real cNIGHT must have at least 1 NIGHT = 1_000_000 STAR
      const nightBalanceInStars = BigInt(status?.nightBalance);
      expect(nightBalanceInStars).toBeGreaterThanOrEqual(STAR_PER_NIGHT);

      // Generation rate should equal nightBalance * GENERATION_DECAY_RATE
      const expectedGenerationRate = nightBalanceInStars * BigInt(GENERATION_DECAY_RATE);
      expect(status?.generationRate).toBe(expectedGenerationRate.toString());

      // Max capacity should equal nightBalance * MAX_SPECK_PER_STAR
      expect(BigInt(status?.maxCapacity)).toBe(BigInt(nightBalanceInStars) * MAX_SPECK_PER_STAR);
      // Current capacity should be less than or equal to max capacity
      expect(BigInt(status?.currentCapacity)).toBeLessThanOrEqual(BigInt(status?.maxCapacity));
    });

    /**
     * A dust generation status query correctly indicates deregistered status for a previously registered
     * Cardano reward address. We won't be able to see the details from this query, in effect the end result
     * will be same as for a non-registered address.
     *
     * This test verifies that when a Cardano reward address was registered for DUST production but has been
     * deregistered, the status correctly reflects that it is no longer registered and all generation values are zero.
     *
     * @given we have a Cardano reward address that was registered for DUST production but has been deregistered
     * @when we query the dust generation status
     * @then the status should indicate registered=false, dustAddress=null, and all generation values should be zero
     */
    test('should correctly indicate deregistered status for a previously registered Cardano reward address', async (ctx: TestContext) => {
      ctx.task!.meta.custom = {
        labels: ['Query', 'Dust', 'Tokenomics', 'cNgD'],
        testKey: 'PM-18415',
      };

      let deregisteredAddress: string;
      try {
        deregisteredAddress = dataProvider.getCardanoRewardAddress('deregistered');
      } catch (error) {
        log.warn(error);
        ctx.skip?.(true, (error as Error).message);
      }

      const response: DustGenerationStatusResponse =
        await indexerHttpClient.getDustGenerationStatus([deregisteredAddress!]);

      expect(response).toBeSuccess();
      const dustGenerationStatus = response.data?.dustGenerationStatus;
      expect(dustGenerationStatus).toBeDefined();
      expect(Array.isArray(dustGenerationStatus)).toBe(true);
      expect(dustGenerationStatus?.length).toBe(1);

      const status = dustGenerationStatus![0];
      // Address is no longer registered for DUST production
      expect(status?.registered).toBe(false);
      expect(status?.dustAddress).toBeNull();

      // All generation values should be zero
      expect(status?.nightBalance).toBe('0');
      expect(status?.generationRate).toBe('0');
      expect(status?.currentCapacity).toBe('0');
      expect(status?.maxCapacity).toBe('0');

      // MappingRemoved clears the stored UTXO reference
      expect(status?.utxoTxHash).toBeNull();
      expect(status?.utxoOutputIndex).toBeNull();
    });
  });

  describe('a dust generation status query with multiple valid Cardano reward addresses', () => {
    /**
     * A dust generation status query with multiple Cardano reward addresses returns multiple statuses
     * given we send the request for multiple addresses (up to 10, which is the limit after which the indexer returns an error)
     *
     * @given we have exactly 10 Cardano reward addresses
     * @when we send a dust generation status query with those addresses
     * @then Indexer should return statuses for each address in the same order
     */
    test('should return statuses for multiple Cardano reward addresses in order', async (ctx: TestContext) => {
      ctx.task!.meta.custom = {
        labels: ['Query', 'Dust', 'Tokenomics', 'cNgD'],
        testKey: 'PM-18410',
      };

      const rewardAddresses = [];
      const maxAcceptedAddresses = 10;

      // To get 10 cardano addresses in the request, we gather 2 ...
      let registeredRewardAddressWithDust: string;
      let registeredRewardAddressWithoutDust: string;
      try {
        registeredRewardAddressWithDust =
          dataProvider.getCardanoRewardAddress('registered-with-dust');
        registeredRewardAddressWithoutDust =
          dataProvider.getCardanoRewardAddress('registered-without-dust');
      } catch (error) {
        log.warn(error);
        ctx.skip?.(true, (error as Error).message);
      }

      // ... and repeat them 5 times so we get 10 addresses in the request
      for (let i = 0; i < maxAcceptedAddresses / 2; i++) {
        rewardAddresses.push(registeredRewardAddressWithDust!);
        rewardAddresses.push(registeredRewardAddressWithoutDust!);
      }
      const response: DustGenerationStatusResponse =
        await indexerHttpClient.getDustGenerationStatus(rewardAddresses);

      expect(response).toBeSuccess();
      expect(response.data?.dustGenerationStatus).toBeDefined();
      expect(response.data?.dustGenerationStatus).toHaveLength(maxAcceptedAddresses);

      // Verify order is preserved
      for (let i = 0; i < rewardAddresses.length; i++) {
        expect(response.data?.dustGenerationStatus[i].cardanoRewardAddress).toBe(
          rewardAddresses[i],
        );
      }
    });

    /**
     * A dust generation status query with multiple reward addresses responds with the expected schema
     *
     * @given we have multiple valid Cardano reward addresses
     * @when we send a dust generation status query with those addresses
     * @then Indexer should respond with dust generation statuses according to the requested schema
     */
    test('should respond with dust generation statuses according to the requested schema for multiple addresses', async (ctx: TestContext) => {
      ctx.task!.meta.custom = {
        labels: ['Query', 'Dust', 'Tokenomics', 'cNgD'],
        testKey: 'PM-18411',
      };

      let registeredRewardAddress: string;
      let nonRegisteredRewardAddress: string;
      try {
        registeredRewardAddress = dataProvider.getCardanoRewardAddress('registered-with-dust');
        nonRegisteredRewardAddress = dataProvider.getCardanoRewardAddress('non-registered');
      } catch (error) {
        log.warn(error);
        ctx.skip?.(true, (error as Error).message);
      }

      const rewardAddresses = [registeredRewardAddress!, nonRegisteredRewardAddress!];
      const response: DustGenerationStatusResponse =
        await indexerHttpClient.getDustGenerationStatus(rewardAddresses);

      expect(response).toBeSuccess();
      expect(response.data!.dustGenerationStatus).toBeDefined();
      expect(Array.isArray(response.data!.dustGenerationStatus)).toBe(true);
      expect(response.data!.dustGenerationStatus).toHaveLength(2);

      // Validate each status against the schema
      for (const status of response.data!.dustGenerationStatus) {
        log.debug(`Validating schema for reward address: ${status.cardanoRewardAddress}`);
        const validationResult = DustGenerationStatusSchema.safeParse(status);
        expect(
          validationResult.success,
          `DUST generation status schema validation failed for ${status.cardanoRewardAddress}: ${JSON.stringify(validationResult.error, null, 2)}`,
        ).toBe(true);
      }
    });

    /**
     * A dust generation status query with multiple reward addresses returns an error when the number of addresses
     * is greater than 10
     *
     * @given we have 11 Cardano reward addresses
     * @when we send a dust generation status query with those addresses
     * @then Indexer should return an error
     */
    test('should return an error given the number of addresses is greater than 10', async (ctx: TestContext) => {
      ctx.task!.meta.custom = {
        labels: ['Query', 'Dust', 'Tokenomics', 'cNgD'],
        testKey: 'PM-18412',
      };

      const rewardAddresses: string[] = [];
      for (let i = 0; i < 11; i++) {
        rewardAddresses.push(generateRewardAddress(i + 1));
      }

      const response: DustGenerationStatusResponse =
        await indexerHttpClient.getDustGenerationStatus(rewardAddresses);

      expect(response).toBeError();
    });
  });

  describe('a dust generation status query with malformed reward addresses', () => {
    /**
     * A dust generation status query with hex string should be rejected as invalid Cardano
     * reward address format
     *
     * @given we provide a Cardano reward address that is in plain hex string format
     * @when we send a dust generation status query
     * @then Indexer should return an error explaining the address format is unexpected and the address is rejected
     */
    test('should return an error when the address is in plain hex string format', async (ctx: TestContext) => {
      ctx.task!.meta.custom = {
        labels: ['Query', 'Dust', 'Tokenomics', 'cNgD'],
        testKey: 'PM-18980',
      };

      const plainHexString =
        '000200e99d4445695a6244a01ab00d592825e2703c3f9a928f01429561585ce2db1e7';

      const response: DustGenerationStatusResponse =
        await indexerHttpClient.getDustGenerationStatus([plainHexString]);
      expect(response).toBeError();
      expect(response.errors?.[0].message).toContain('invalid Cardano reward address');
    });
  });

  describe('a dust generation status query with empty list of reward addresses', () => {
    /**
     * A dust generation status with empty array should return an empty array
     *
     * @given we provide an empty array of Cardano reward addresses
     * @when we send a dust generation status query
     * @then Indexer should return an empty array
     */
    test('should return an empty list of dust generation statuses', async (ctx: TestContext) => {
      ctx.task!.meta.custom = {
        labels: ['Query', 'Dust', 'Tokenomics', 'cNgD'],
        testKey: 'PM-18981',
      };

      const response: DustGenerationStatusResponse =
        await indexerHttpClient.getDustGenerationStatus([]);

      // The API should either return an empty array or an error
      // We check for both possibilities
      if (response.errors) {
        expect(response).toBeError();
      } else {
        expect(response).toBeSuccess();
        expect(response.data?.dustGenerationStatus).toBeDefined();
        expect(response.data?.dustGenerationStatus).toHaveLength(0);
      }
    });
  });

  describe('a dust generation status query with a Cardano payment address', () => {
    /**
     * A dust generation status query for a Cardano payment address should repond with an error
     *
     * @given we have a Cardano payment address (i.e. with addr or addr_test HRP)
     * @when we send a dust generation status query with that address
     * @then Indexer should return an error explaining the address format is unexpected
     */
    test('should return an error as only Cardano reward addresses are supported', async (ctx: TestContext) => {
      ctx.task!.meta.custom = {
        labels: ['Query', 'Dust', 'Tokenomics', 'cNgD'],
        testKey: 'PM-18983',
      };

      const paymentAddress = 'addr_test1u80zp3ht7500msrwhcawj4jrqegv9fwhww4k3e95j7dntrq54lptp';
      const response: DustGenerationStatusResponse =
        await indexerHttpClient.getDustGenerationStatus([paymentAddress]);
      expect(response).toBeError();
      expect(response.errors?.[0].message).toContain('invalid Cardano reward address');
    });
  });

  describe('a dust generation status query with a Cardano reward address not meant for this network', () => {
    /**
     * A dust generation status query for a Cardano reward address not meant for this network
     * should repond with an error
     *
     * NOTE: we are talking about Cardano network mismatch, and we need to consider the connection between
     * Cardano networks and Midnight environments. This is the mapping we are using:
     * - Cardano mainnet(mainnet) -> Midnight mainnet
     * - Cardano preprod(testnet) -> Midnight preprod
     * - Cardano preview(testnet) -> Midnight preview
     * -         |                -> Midnight qanet
     * -         |                -> Midnight devnet
     *
     * So essntially to check the expected encoding, it's enough to check the Midnight environment name, given
     * the mapping above. Also note that preprod and preview addresses in Cardano have the test label in the
     * HRP, whilst mainnet addresses do not.
     *
     * @given we have a Cardano reward address not meant for this network
     * @when we send a dust generation status query with that address
     * @then Indexer should return an error reporting the target network mismatch
     */
    test('should return an error reporting the target network mismatch', async (ctx: TestContext) => {
      ctx.task!.meta.custom = {
        labels: ['Query', 'Dust', 'Tokenomics', 'cNgD'],
        testKey: 'PM-18911',
      };

      let rewardAddress: string;
      const connectedCardanoNetworkType = env.getCardanoNetworkType();

      // We need to use Cardano reward address that is not meant for the connected Cardano network
      // to validate indexer rejects it as invalid.
      if (connectedCardanoNetworkType === 'mainnet') {
        rewardAddress = 'stake_test1uqlkhzj4uqvl7x7q4qccgcmvyvjxa9xym4zvcmgemgltwnqt77qfc';
      } else {
        rewardAddress = 'stake1ux0k2hy4h6c8k95vzr52ant8yy77ggxg2wmk7cha4h4kraqjq4sfe';
      }

      log.debug(`Using cross-network Cardano reward address: ${rewardAddress}`);
      const response: DustGenerationStatusResponse =
        await indexerHttpClient.getDustGenerationStatus([rewardAddress]);
      log.debug(`Dust generation status response payload: ${JSON.stringify(response.data)}`);
      expect(response).toBeError();

      expect(response.errors?.[0].message).toContain('invalid Cardano reward address');
    });
  });

  describe('a dust generation status query with duplicate reward addresses', () => {
    /**
     * A dust generation status query with duplicate Cardano reward addresses returns status for each occurrence
     *
     * @given we provide duplicate Cardano reward addresses in the array
     * @when we send a dust generation status query
     * @then Indexer should return status for each occurrence in the same order
     */
    test('should handle duplicate Cardano reward addresses appropriately', async (ctx: TestContext) => {
      ctx.task!.meta.custom = {
        labels: ['Query', 'Dust', 'Tokenomics', 'cNgD'],
        testKey: 'PM-18982',
      };

      let registeredRewardAddress: string;
      try {
        registeredRewardAddress = dataProvider.getCardanoRewardAddress('registered-with-dust');
      } catch (error) {
        log.warn(error);
        ctx.skip?.(true, (error as Error).message);
      }

      // Send the same address twice
      const duplicateRewardAddresses = [registeredRewardAddress!, registeredRewardAddress!];
      const response: DustGenerationStatusResponse =
        await indexerHttpClient.getDustGenerationStatus(duplicateRewardAddresses);

      expect(response).toBeSuccess();
      expect(response.data?.dustGenerationStatus).toBeDefined();
      expect(response.data?.dustGenerationStatus).toHaveLength(2);

      // Both results should have the same reward address
      expect(response.data?.dustGenerationStatus[0].cardanoRewardAddress.toLowerCase()).toBe(
        registeredRewardAddress!.toLowerCase(),
      );
      expect(response.data?.dustGenerationStatus[1].cardanoRewardAddress.toLowerCase()).toBe(
        registeredRewardAddress!.toLowerCase(),
      );
    });
  });
});
