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
import { env } from 'environment/model';
import { randomBytes } from 'node:crypto';
import '@utils/logging/test-logging-hooks';
import { ToolkitWrapper } from '@utils/toolkit/toolkit-wrapper';

describe('key material derivation validation', () => {
  let toolkit: ToolkitWrapper;
  const seed = randomBytes(32).toString('hex');

  beforeAll(async () => {
    toolkit = new ToolkitWrapper({});
    await toolkit.start();
  });

  afterAll(async () => {
    await toolkit.stop();
  });

  describe('a midnight shielded address', () => {
    /**
     * Test that the shielded address shows with the expected prefix for the current network ID
     *
     * @given a randomly generated seed
     * @when we show the shielded address for the current network ID
     * @then the address should start with the expected prefix
     */
    test('should show with the expected prefix for the current network ID', async () => {
      const address = (await toolkit.showAddress(seed)).shielded;

      log.info(`Shielded address: ${address}`);

      // Mainnet addresses do not have a network ID prefix
      const networkId = env.getNetworkId();
      const addressPrefix =
        networkId === 'mainnet' ? `mn_shield-addr` : `mn_shield-addr_${networkId}`;
      expect(address).toMatch(new RegExp(`^${addressPrefix}`));
    });

    /**
     * Test that the shielded address shows with the expected prefix for all network IDs
     *
     * @given a randomly generated seed
     * @when we show the shielded address for all network IDs
     * @then the address should start with the expected prefix
     */
    test('should show with the expected prefix for all network IDs', async () => {
      // For all known environments check if the right prefix is present (prefix uses network ID)
      const environmentNames = env.getAllEnvironmentNames();
      for (const envName of environmentNames) {
        const address = (await toolkit.showAddress(seed, envName)).shielded;
        log.info(`Shielded address: ${address}`);

        // Address prefix uses network ID
        const networkId = envName;
        const addressPrefix =
          networkId === 'mainnet' ? `mn_shield-addr` : `mn_shield-addr_${networkId}`;
        expect(address).toMatch(new RegExp(`^${addressPrefix}`));
      }
    });
  });

  describe('a midnight unshielded address', () => {
    /**
     * Test that the unshielded address shows with the expected prefix for the current network ID
     *
     * @given a randomly generated seed
     * @when we show the unshielded address for the current network ID
     * @then the address should start with the expected prefix
     */
    test('should show with the expected prefix for the current network ID', async () => {
      const address = (await toolkit.showAddress(seed)).unshielded;

      log.info(`Unshielded address: ${address}`);

      // Mainnet addresses do not have a network ID prefix
      const networkId = env.getNetworkId();
      const addressPrefix = networkId === 'mainnet' ? `mn_addr` : `mn_addr_${networkId}`;
      expect(address).toMatch(new RegExp(`^${addressPrefix}`));
    });

    /**
     * Test that the unshielded address shows with the expected prefix for all network IDs
     *
     * @given a randomly generated seed
     * @when we show the unshielded address for all network IDs
     * @then the address should start with the expected prefix
     */
    test('should show with the expected prefix for all network IDs', async () => {
      // For all known environments check if the right prefix is present (prefix uses network ID)
      const environmentNames = env.getAllEnvironmentNames();
      for (const envName of environmentNames) {
        const address = (await toolkit.showAddress(seed, envName)).unshielded;
        log.info(`Unshielded address: ${address}`);

        // Address prefix uses network ID
        const networkId = envName;
        const addressPrefix = networkId === 'mainnet' ? `mn_addr` : `mn_addr_${networkId}`;
        expect(address).toMatch(new RegExp(`^${addressPrefix}`));
      }
    });
  });

  describe('a midnight viewing key', () => {
    /**
     * Test that the viewing key shows with the expected prefix for the current network ID
     *
     * @given a randomly generated seed
     * @when we show the viewing key for the current network ID
     * @then the viewing key should start with the expected prefix
     */
    test('should show with the expected prefix for the current network ID', async () => {
      const viewingKey = await toolkit.showViewingKey(seed);

      log.info(`Viewing key: ${viewingKey}`);

      const addressPrefix = 'mn_shield-esk_';
      expect(viewingKey).toMatch(new RegExp(`^${addressPrefix}`));
    });

    /**
     * Test that the viewing key shows with the expected prefix for all network IDs
     *
     * @given a randomly generated seed
     * @when we show the viewing key for all network IDs
     * @then the viewing key should start with the expected prefix
     */
    test('should show with the expected prefix for all network IDs', async () => {
      // For all known environments check if the right prefix is present (prefix uses network ID)
      const environmentNames = env.getAllEnvironmentNames();
      for (const envName of environmentNames) {
        const address = await toolkit.showViewingKey(seed, envName);
        log.info(`Viewing key for ${envName}: ${address}`);

        // Viewing key prefix uses network ID
        const networkId = envName;
        const addressPrefix =
          networkId === 'mainnet' ? `mn_shield-esk` : `mn_shield-esk_${networkId}`;
        expect(address).toMatch(new RegExp(`^${addressPrefix}`));
      }
    });
  });
});
