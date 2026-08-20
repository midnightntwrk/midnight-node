// This file is part of midnightntwrk/midnight-indexer
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

// Coverage for #1253 (deploy minter → mint N to self → send M < N → assert the
// indexer reports the contract's N - M remainder via unshieldedBalances). This is
// the full version of the #1245 reporter's scenario.
//
// Kept describe.skip pending one validated run on a ledger-9 toolkit+node stack.
// The two original TODOs are now resolved: the compiled minter contract lives in
// the toolkit image (paths below, verified against
// midnightntwrk/midnight-node-toolkit), and the ledger-9 toolchain blocker was
// lifted by midnight-node#1711 (compact 0.33.0-rc.1 / Ledger 9). Un-skip once the
// e2e has been run green with the node and toolkit both on the ledger-9 line.

import type { TestContext } from 'vitest';
import '@utils/logging/test-logging-hooks';
import dataProvider from '@utils/testdata-provider';
import { retry } from '@utils/retry-helper';
import { IndexerHttpClient } from '@utils/indexer/http-client';
import { ToolkitWrapper, type MinterContract } from '@utils/toolkit/toolkit-wrapper';

const TOOLKIT_WRAPPER_TIMEOUT = 120_000;
const CONTRACT_ACTION_TIMEOUT = 180_000;
const TEST_TIMEOUT = 30_000;

// Compiled minter contract paths as seen inside the toolkit container. The image
// ships the contract at /toolkit-js/test/minter_contract (source, config and the
// compiled out/ dir); the node script (toolkit-tokens-minter-e2e.sh) copies it
// from there, and since ToolkitWrapper execs inside the image it is referenced
// in place — no bind mount needed.
const MINTER_CONTRACT: MinterContract = {
  compiledContractDir: '/toolkit-js/test/minter_contract/out',
  configFile: '/toolkit-js/test/minter_contract/minter.config.ts',
  toolkitJsPath: '/toolkit-js',
};

const DOMAIN_SEP = 'feeb000000000000000000000000000000000000000000000000000000000000';
const MINT_AMOUNT = 1000;
const SEND_AMOUNT = 400; // remainder the contract should still hold: 600

describe.skip('unshielded balance indexing (deploy + mint + verify) [#1253 scaffold]', () => {
  let indexerHttpClient: IndexerHttpClient;
  let toolkit: ToolkitWrapper;
  let fundingSeed: string;

  beforeAll(async () => {
    indexerHttpClient = new IndexerHttpClient();
    fundingSeed = dataProvider.getFundingSeed();
    toolkit = new ToolkitWrapper({});
    await toolkit.start();
  }, TOOLKIT_WRAPPER_TIMEOUT);

  afterAll(async () => {
    await toolkit.stop();
  });

  test(
    'reports the contract remainder after mint-to-self then partial send',
    async (context: TestContext) => {
      context.task!.meta.custom = {
        labels: ['ContractAction', 'unshieldedBalances', 'Mint', 'E2E'],
      };

      // Deploy → mint N to self → send M to user.
      const result = await toolkit.deployMintSendUnshielded({
        contract: MINTER_CONTRACT,
        domainSep: DOMAIN_SEP,
        mintAmount: MINT_AMOUNT,
        sendAmount: SEND_AMOUNT,
        fundingSeed,
      });

      // Poll until the indexer has caught up to the mint+send transaction: the
      // contract action carries the minted token's remainder in unshieldedBalances.
      const held = await retry(
        async () => {
          const response = await indexerHttpClient.getContractAction(result.contractAddress);
          const balances = response.data?.contractAction?.unshieldedBalances ?? [];
          const entry = balances.find((balance) => balance.tokenType === result.tokenType);
          if (!entry) {
            throw new Error(
              `indexer has not yet reported token ${result.tokenType} for ${result.contractAddress}`,
            );
          }
          return entry;
        },
        { maxRetries: 10, delayMs: 3000, retryLabel: 'indexer catch-up for minted balance' },
      );

      expect(held).toBeDefined();
      expect(held.amount).toBe(String(result.expectedRemainder));
    },
    CONTRACT_ACTION_TIMEOUT + TEST_TIMEOUT,
  );
});
