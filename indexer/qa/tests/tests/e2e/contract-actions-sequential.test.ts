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

import type { TestContext } from 'vitest';
import '@utils/logging/test-logging-hooks';
import log from '@utils/logging/logger';
import dataProvider from '@utils/testdata-provider';
import { IndexerHttpClient } from '@utils/indexer/http-client';
import { getBlockByHashWithRetry, getTransactionByHashWithRetry } from './test-utils';
import {
  ToolkitWrapper,
  DeployContractResult,
  ToolkitTransactionResult,
} from '@utils/toolkit/toolkit-wrapper';
import { Transaction } from '@utils/indexer/indexer-types';

const TOOLKIT_WRAPPER_TIMEOUT = 120_000; // 2 minutes
const CONTRACT_ACTION_TIMEOUT = 150_000; // 2.5 minutes
const TEST_TIMEOUT = 10_000; // 10 seconds

/** Normalize hash for comparison (indexer may return with or without 0x prefix). */
function sameHash(a: string | undefined, b: string | undefined): boolean {
  const n = (h: string | undefined) => (h ?? '').trim().toLowerCase().replace(/^0x/, '');
  return n(a) === n(b);
}

describe.sequential('contract actions', () => {
  let indexerHttpClient: IndexerHttpClient;
  let toolkit: ToolkitWrapper;
  let fundingSeed: string;
  let contractDeployResult: DeployContractResult;
  let contractCallResult: ToolkitTransactionResult;
  let contractUpdateResult: ToolkitTransactionResult;

  let contractCallBlockHash: string;
  let contractCallTransactionHash: string;

  let contractUpdateBlockHash: string;
  let contractUpdateTransactionHash: string;

  beforeAll(async () => {
    indexerHttpClient = new IndexerHttpClient();
    fundingSeed = dataProvider.getFundingSeed();
    toolkit = new ToolkitWrapper({});
    await toolkit.start();
  }, TOOLKIT_WRAPPER_TIMEOUT);

  afterAll(async () => {
    await toolkit.stop();
  });

  describe('a transaction to deploy a smart contract', () => {
    beforeAll(async () => {
      contractDeployResult = await toolkit.deployContract(fundingSeed);
    }, CONTRACT_ACTION_TIMEOUT);

    /**
     * Once a contract deployment transaction has been submitted to node and confirmed, the indexer should report
     * that transaction through a transaction query by hash, using the transaction hash reported by the toolkit.
     *
     * @given a confirmed contract deployment transaction
     * @when we query the indexer with a transaction query by hash, using the transaction hash reported by the toolkit
     * @then the transaction should be found and reported correctly
     */
    test(
      'should be reported by the indexer through a transaction query by hash',
      async (context: TestContext) => {
        context.task!.meta.custom = {
          labels: ['Query', 'Transaction', 'ByHash', 'ContractDeploy'],
        };

        const deployTxHash = contractDeployResult['deploy-tx-hash'];
        const transactionResponse = await getTransactionByHashWithRetry(deployTxHash);

        // Verify the transaction appears in the response
        expect(transactionResponse).toBeSuccess();
        expect(transactionResponse?.data?.transactions).toBeDefined();
        expect(transactionResponse?.data?.transactions?.length).toBeGreaterThan(0);

        const foundTransaction = transactionResponse.data?.transactions?.find((tx: Transaction) =>
          sameHash(tx.hash, deployTxHash),
        );
        expect(foundTransaction).toBeDefined();
        expect(sameHash(foundTransaction?.hash, deployTxHash)).toBe(true);
      },
      TEST_TIMEOUT,
    );

    /**
     * Once a contract deployment transaction has been submitted to node and confirmed, the indexer should report
     * that transaction in the block through a block query by hash, using the block hash reported by the toolkit.
     *
     * @given a confirmed contract deployment transaction
     * @when we query the indexer with a block query by hash, using the block hash reported by the toolkit
     * @then the block should contain the contract deployment transaction
     */
    test(
      'should be reported by the indexer through a block query by hash',
      async (context: TestContext) => {
        context.task!.meta.custom = {
          labels: ['Query', 'Block', 'ByHash', 'ContractDeploy'],
        };

        const deployTxHash = contractDeployResult['deploy-tx-hash'];
        const deployBlockHash = contractDeployResult['deploy-block-hash'];
        const blockResponse = await getBlockByHashWithRetry(deployBlockHash);

        // Verify the block appears in the response
        expect(blockResponse).toBeSuccess();
        expect(blockResponse?.data?.block).toBeDefined();
        expect(blockResponse?.data?.block?.transactions).toBeDefined();
        expect(blockResponse?.data?.block?.transactions?.length).toBeGreaterThan(0);

        const foundTransaction = blockResponse.data?.block?.transactions?.find((tx: Transaction) =>
          sameHash(tx.hash, deployTxHash),
        );
        expect(sameHash(foundTransaction?.hash, deployTxHash)).toBe(true);
        expect(sameHash(blockResponse.data?.block?.hash, deployBlockHash)).toBe(true);
      },
      TEST_TIMEOUT,
    );

    /**
     * Once a contract deployment transaction has been submitted to node and confirmed, the indexer should report
     * the contract action with the correct type when queried by contract address.
     *
     * @given a confirmed contract deployment transaction
     * @when we query the indexer with a contract action query by address
     * @then the contract action should be found with __typename 'ContractDeploy'
     */
    test(
      'should be reported by the indexer through a contract action query by address',
      async (context: TestContext) => {
        context.task!.meta.custom = {
          labels: ['Query', 'ContractAction', 'ByAddress', 'ContractDeploy'],
        };

        // Query the contract action by address (using the contract address for GraphQL queries)
        const contractActionResponse = await indexerHttpClient.getContractAction(
          contractDeployResult['contract-address-untagged'],
        );

        // Verify the contract action appears in the response
        expect(contractActionResponse?.data?.contractAction).toBeDefined();

        const contractAction = contractActionResponse.data?.contractAction;
        expect(contractAction?.__typename).toBe('ContractDeploy');

        if (contractAction?.__typename === 'ContractDeploy') {
          expect(contractAction.address).toBeDefined();
          expect(
            sameHash(contractAction.address, contractDeployResult['contract-address-untagged']),
          ).toBe(true);

          const zswapState = contractAction.zswapState;
          log.debug(`zswapState (Deploy): length ${zswapState?.length ?? 0}`);
          expect.soft(zswapState).toBeDefined();
          expect.soft(typeof zswapState).toBe('string');
          expect.soft(zswapState?.length ?? 0).toBeGreaterThan(0);
        }
      },
      TEST_TIMEOUT,
    );
  });

  describe('a transaction to call a smart contract', () => {
    beforeAll(async () => {
      contractCallResult = await toolkit.callContract(
        'store',
        contractDeployResult,
        undefined,
        fundingSeed,
      );

      expect(contractCallResult.status).toBe('confirmed');
      log.debug(
        `Call tx hash: ${contractCallResult.txHash}, block: ${contractCallResult.blockHash}`,
      );

      contractCallBlockHash = contractCallResult.blockHash;
      contractCallTransactionHash = contractCallResult.txHash;
    }, CONTRACT_ACTION_TIMEOUT);

    /**
     * Once a contract call transaction has been submitted to node and confirmed, the indexer should report
     * that transaction through a query by transaction hash, using the transaction hash reported by the toolkit.
     *
     * @given a confirmed contract call transaction
     * @when we query the indexer with a transaction query by hash, using the transaction hash reported by the toolkit
     * @then the transaction should be found and reported correctly
     */
    test(
      'should be reported by the indexer through a transaction query by hash',
      async (context: TestContext) => {
        context.task!.meta.custom = {
          labels: ['Query', 'Transaction', 'ByHash', 'ContractCall'],
        };

        const transactionResponse = await getTransactionByHashWithRetry(
          contractCallTransactionHash,
        );

        expect(transactionResponse).toBeSuccess();
        expect(transactionResponse?.data?.transactions).toBeDefined();
        expect(transactionResponse?.data?.transactions?.length).toBeGreaterThan(0);

        const foundTransaction = transactionResponse.data?.transactions?.find((tx: Transaction) =>
          sameHash(tx.hash, contractCallTransactionHash),
        );
        expect(foundTransaction).toBeDefined();
        expect(sameHash(foundTransaction?.hash, contractCallTransactionHash)).toBe(true);
      },
      TEST_TIMEOUT,
    );

    /**
     * Once a contract call transaction has been submitted to node and confirmed, the indexer should report
     * that transaction in the block through a block query by hash, using the block hash reported by the toolkit.
     *
     * @given a confirmed contract call transaction
     * @when we query the indexer with a block query by hash, using the block hash reported by the toolkit
     * @then the block should contain the contract call transaction
     */
    test(
      'should be reported by the indexer through a block query by hash',
      async (context: TestContext) => {
        context.task!.meta.custom = {
          labels: ['Query', 'Block', 'ByHash', 'ContractCall'],
        };

        const blockResponse = await getBlockByHashWithRetry(contractCallBlockHash);

        expect(blockResponse).toBeSuccess();
        expect(blockResponse.data?.block).toBeDefined();
        expect(sameHash(blockResponse.data?.block?.hash, contractCallBlockHash)).toBe(true);
      },
      TEST_TIMEOUT,
    );

    /**
     * Once a contract call transaction has been submitted to node and confirmed, the indexer should report
     * the contract action with the correct type when queried by contract address.
     *
     * @given a confirmed contract call transaction
     * @when we query the indexer with a contract action query by address
     * @then the contract action should be found with __typename 'ContractCall'
     */
    test(
      'should be reported by the indexer through a contract action query by address',
      async (context: TestContext) => {
        context.task!.meta.custom = {
          labels: ['Query', 'ContractAction', 'ByAddress', 'ContractCall'],
        };

        // Query the contract action by address (using the contract address for GraphQL queries)
        const contractActionResponse = await indexerHttpClient.getContractAction(
          contractDeployResult['contract-address-untagged'],
        );

        // Verify the contract action appears in the response
        expect(contractActionResponse?.data?.contractAction).toBeDefined();

        const contractAction = contractActionResponse.data?.contractAction;
        expect(contractAction?.__typename).toBe('ContractCall');

        if (contractAction?.__typename === 'ContractCall') {
          expect(contractAction.address).toBeDefined();
          expect(
            sameHash(contractAction.address, contractDeployResult['contract-address-untagged']),
          ).toBe(true);
          expect(contractAction.entryPoint).toBeDefined();
          expect(contractAction.deploy).toBeDefined();
          expect(contractAction.deploy?.address).toBeDefined();
        }
      },
      TEST_TIMEOUT,
    );
  });

  describe('a transaction to update a smart contract', () => {
    beforeAll(async () => {
      // Allow call to finalize before running maintenance (update)
      await getTransactionByHashWithRetry(contractCallTransactionHash);
      await new Promise((resolve) => setTimeout(resolve, 30000));
      contractUpdateResult = await toolkit.updateContract(contractDeployResult, fundingSeed);

      expect(contractUpdateResult.status).toBe('confirmed');

      contractUpdateBlockHash = contractUpdateResult.blockHash;
      contractUpdateTransactionHash = contractUpdateResult.txHash;
    }, CONTRACT_ACTION_TIMEOUT);

    /**
     * Once a contract update (maintenance) transaction has been submitted and confirmed, the indexer
     * should report that transaction via a transaction query by hash.
     *
     * @given a confirmed contract update transaction
     * @when we query the indexer by transaction hash (from the toolkit)
     * @then the indexer returns the update transaction
     */
    test(
      'should be reported by the indexer through a transaction query by hash',
      async (context: TestContext) => {
        context.task!.meta.custom = {
          labels: ['Query', 'Transaction', 'ByHash', 'ContractUpdate'],
        };

        const transactionResponse = await getTransactionByHashWithRetry(
          contractUpdateTransactionHash,
        );

        expect(transactionResponse).toBeSuccess();
        expect(transactionResponse?.data?.transactions).toBeDefined();
        expect(transactionResponse?.data?.transactions?.length).toBeGreaterThan(0);

        const foundTransaction = transactionResponse.data?.transactions?.find((tx: Transaction) =>
          sameHash(tx.hash, contractUpdateTransactionHash),
        );

        expect(foundTransaction).toBeDefined();
        expect(sameHash(foundTransaction?.hash, contractUpdateTransactionHash)).toBe(true);
      },
      TEST_TIMEOUT,
    );

    /**
     * Once a contract update (maintenance) transaction has been submitted and confirmed, the indexer
     * should report that transaction in the block via a block query by hash.
     *
     * @given a confirmed contract update transaction
     * @when we query the indexer by block hash (from the toolkit)
     * @then the block contains the update transaction
     */
    test(
      'should be reported by the indexer through a block query by hash',
      async (context: TestContext) => {
        context.task!.meta.custom = {
          labels: ['Query', 'Block', 'ByHash', 'ContractUpdate'],
        };

        await getTransactionByHashWithRetry(contractUpdateTransactionHash);

        const blockResponse = await getBlockByHashWithRetry(contractUpdateBlockHash);

        expect(blockResponse).toBeSuccess();
        expect(blockResponse?.data?.block).toBeDefined();
        expect(sameHash(blockResponse?.data?.block?.hash, contractUpdateBlockHash)).toBe(true);
        expect(blockResponse?.data?.block?.transactions).toBeDefined();
        expect(blockResponse?.data?.block?.transactions?.length).toBeGreaterThan(0);

        const foundUpdateTx = blockResponse.data?.block?.transactions?.find((tx: Transaction) =>
          sameHash(tx.hash, contractUpdateTransactionHash),
        );
        expect(foundUpdateTx).toBeDefined();
        expect(sameHash(foundUpdateTx?.hash, contractUpdateTransactionHash)).toBe(true);
      },
      TEST_TIMEOUT,
    );

    /**
     * Once a contract update (maintenance) has been submitted and confirmed, the indexer should
     * report the latest contract action as ContractUpdate when queried by contract address.
     *
     * @given a confirmed contract update transaction
     * @when we query the indexer for contract action by address
     * @then the contract action has __typename 'ContractUpdate'
     */
    test(
      'should be reported by the indexer through a contract action query by address',
      async (context: TestContext) => {
        context.task!.meta.custom = {
          labels: ['Query', 'ContractAction', 'ByAddress', 'ContractUpdate'],
        };

        await getTransactionByHashWithRetry(contractUpdateTransactionHash);

        const contractActionResponse = await indexerHttpClient.getContractAction(
          contractDeployResult['contract-address-untagged'],
        );

        expect(contractActionResponse?.data?.contractAction).toBeDefined();

        const contractAction = contractActionResponse.data?.contractAction;
        expect(contractAction?.__typename).toBe('ContractUpdate');

        if (contractAction?.__typename === 'ContractUpdate') {
          expect(contractAction.address).toBeDefined();
          expect(
            sameHash(contractAction.address, contractDeployResult['contract-address-untagged']),
          ).toBe(true);
        }
      },
      TEST_TIMEOUT,
    );
  });
});
