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
import type { TestContext } from 'vitest';
import '@utils/logging/test-logging-hooks';
import dataProvider from '@utils/testdata-provider';
import { ToolkitWrapper } from '@utils/toolkit/toolkit-wrapper';
import {
  type PrivateWalletState,
  type PublicWalletState,
  PrivateWalletStateSchema,
  PublicWalletStateSchema,
} from '@utils/toolkit/schemas';

const TOOLKIT_STARTUP_TIMEOUT = 60_000;

describe('show wallet queries using toolkit', () => {
  let toolkit: ToolkitWrapper;

  beforeAll(async () => {
    toolkit = new ToolkitWrapper({});
    await toolkit.start();
  }, TOOLKIT_STARTUP_TIMEOUT);

  afterAll(async () => {
    await toolkit.stop();
  });

  describe('private wallet state query using toolkit', () => {
    /**
     * A private wallet state query using the toolkit's showPrivateWalletState method should return
     * a valid private wallet state object according to the requested schema.
     *
     * @given we have a toolkit instance and a wallet seed
     * @when we call showPrivateWalletState with the seed
     * @then we should receive a valid PrivateWalletState object according to the requested schema
     */
    test(
      'should respond with a private wallet state according to the requested schema',
      async (ctx: TestContext) => {
        ctx.task!.meta.custom = {
          labels: ['Query', 'Wallet', 'Toolkit', 'PrivateState', 'SchemaValidation'],
        };

        const walletSeed = dataProvider.getFundingSeed();

        log.debug(`Querying private wallet state for seed: ${walletSeed}`);
        const walletState: PrivateWalletState = await toolkit.showPrivateWalletState(walletSeed);

        expect(walletState).toBeDefined();

        const validationResult = PrivateWalletStateSchema.safeParse(walletState);
        expect(
          validationResult.success,
          `PrivateWalletState validation failed: ${JSON.stringify(validationResult.error, null, 2)}`,
        ).toBe(true);
      },
      TOOLKIT_STARTUP_TIMEOUT,
    );
  });

  describe('public wallet state query using toolkit', () => {
    /**
     * A public wallet state query using the toolkit's showPublicWalletState method should return
     * a valid public wallet state object according to the requested schema.
     *
     * @given we have a toolkit instance and a wallet address
     * @when we call showPublicWalletState with the address
     * @then we should receive a valid PublicWalletState object according to the requested schema
     */
    test(
      'should respond with a public wallet state according to the requested schema',
      async (ctx: TestContext) => {
        ctx.task!.meta.custom = {
          labels: ['Query', 'Wallet', 'Toolkit', 'PublicState', 'SchemaValidation'],
        };

        const walletSeed = dataProvider.getFundingSeed();

        // Get the unshielded address from the seed
        const addressInfo = await toolkit.showAddress(walletSeed);
        const walletAddress = addressInfo.unshielded;

        log.debug(`Querying public wallet state for address: ${walletAddress}`);
        const publicWalletState: PublicWalletState =
          await toolkit.showPublicWalletState(walletAddress);

        expect(publicWalletState).toBeDefined();

        const validationResult = PublicWalletStateSchema.safeParse(publicWalletState);
        expect(
          validationResult.success,
          `PublicWalletState validation failed: ${JSON.stringify(validationResult.error, null, 2)}`,
        ).toBe(true);
      },
      TOOLKIT_STARTUP_TIMEOUT,
    );
  });
});
