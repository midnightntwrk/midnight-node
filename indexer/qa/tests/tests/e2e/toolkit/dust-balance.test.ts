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
import { ToolkitWrapper, type DustBalance } from '@utils/toolkit/toolkit-wrapper';
import { DustBalanceSchema } from '@utils/toolkit/schemas';

const TOOLKIT_STARTUP_TIMEOUT = 60_000;

describe('dust balance query using toolkit', () => {
  let toolkit: ToolkitWrapper;

  beforeAll(async () => {
    toolkit = new ToolkitWrapper({});
    await toolkit.start();
  }, TOOLKIT_STARTUP_TIMEOUT);

  afterAll(async () => {
    await toolkit.stop();
  });

  describe('a dust balance query with a valid wallet seed', () => {
    /**
     * A dust balance query using the toolkit's getDustBalance method should return
     * a valid dust balance object according to the requested schema.
     *
     * @given we have a toolkit instance and a wallet seed
     * @when we call getDustBalance with the seed
     * @then we should receive a valid DustBalance object according to the requested schema
     */
    test(
      'should respond with a dust balance according to the requested schema',
      async (ctx: TestContext) => {
        ctx.task!.meta.custom = {
          labels: ['Query', 'Dust', 'Toolkit', 'Balance', 'SchemaValidation'],
        };

        const walletSeed = dataProvider.getFundingSeed();

        log.debug(`Querying dust balance for seed: ${walletSeed}`);

        const dustBalance: DustBalance = await toolkit.getDustBalance(walletSeed);

        log.debug('Checking if we actually received a dust balance');
        expect(dustBalance).toBeDefined();

        const validationResult = DustBalanceSchema.safeParse(dustBalance);
        expect(
          validationResult.success,
          `DUST balance schema validation failed: ${JSON.stringify(validationResult.error, null, 2)}`,
        ).toBe(true);

        expect(dustBalance.total).toBeGreaterThan(0);
        log.debug(`Dust balance total: ${dustBalance.total}`);
      },
      TOOLKIT_STARTUP_TIMEOUT,
    );
  });
});
