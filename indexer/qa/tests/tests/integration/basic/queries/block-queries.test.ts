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
import { BlockSchema } from '@utils/indexer/graphql/schema';
import { IndexerHttpClient } from '@utils/indexer/http-client';
import { isPerBlockDustRootsSupported } from '@utils/indexer/schema-feature-probe';
import type {
  Block,
  BlockResponse,
  RegularTransaction,
  Transaction,
} from '@utils/indexer/indexer-types';
import dataProvider from '@utils/testdata-provider';
import { TestContext } from 'vitest';

const indexerHttpClient = new IndexerHttpClient();

// Utility function to get a block by hash given we extract
// the hash from the latest block. This function has been
// created to avoid code duplication and to make the tests more readable.
async function getLatestBlockByHash(): Promise<Block> {
  log.debug('Requesting latest block from indexer');
  const response: BlockResponse = await indexerHttpClient.getLatestBlock();
  expect(response).toBeSuccess();
  expect(response.data?.block).toBeDefined();
  expect(response.data?.block?.hash).toBeDefined();

  const latestBlockHash = response.data?.block?.hash;
  log.debug(`Requesting block by hash = ${latestBlockHash}`);
  const blockByHashResponse: BlockResponse = await indexerHttpClient.getBlockByOffset({
    hash: latestBlockHash,
  });
  expect(blockByHashResponse).toBeSuccess();
  expect(blockByHashResponse.data?.block).toBeDefined();
  expect(blockByHashResponse.data?.block?.hash).toBeDefined();
  expect(blockByHashResponse.data?.block?.hash).toBe(latestBlockHash);

  expect(blockByHashResponse.data?.block?.ledgerParameters).toBeTruthy();

  return blockByHashResponse.data?.block as Block;
}

// Utility function to get a block by height given we extract
// the height from the latest block. This function has been
// created to avoid code duplication and to make the tests more readable.
async function getLatestBlockByHeight(): Promise<Block> {
  log.debug('Requesting latest block from indexer');
  const response: BlockResponse = await indexerHttpClient.getLatestBlock();
  expect(response).toBeSuccess();
  expect(response.data?.block).toBeDefined();
  expect(response.data?.block?.hash).toBeDefined();

  const latestBlockHeight = response.data?.block?.height;
  log.debug(`Requesting block by height = ${latestBlockHeight}`);
  const blockByHashResponse: BlockResponse = await indexerHttpClient.getBlockByOffset({
    height: latestBlockHeight,
  });
  expect(blockByHashResponse).toBeSuccess();
  expect(blockByHashResponse.data?.block).toBeDefined();
  expect(blockByHashResponse.data?.block?.height).toBeDefined();
  expect(blockByHashResponse.data?.block?.height).toBe(latestBlockHeight);

  expect(blockByHashResponse.data?.block?.ledgerParameters).toBeTruthy();

  return blockByHashResponse.data?.block as Block;
}

describe('block queries', () => {
  describe('a block query without parameters', () => {
    /**
     * A block query without parameters returns the latest block
     *
     * @when we send a block query without parameters
     * @then Indexer should return the latest known block
     */
    test('should return the latest known block', async (ctx: TestContext) => {
      ctx.task!.meta.custom = {
        labels: ['Query', 'Block', 'Latest'],
        testKey: 'PM-17677',
      };

      log.debug('Requesting latest block from indexer');
      const response: BlockResponse = await indexerHttpClient.getLatestBlock();

      expect(response).toBeSuccess();
      expect(response.data?.block).toBeDefined();

      const latestBlock = response.data!.block;
      const latestHeight = latestBlock.height;
      const nextBlockResponse = await indexerHttpClient.getBlockByOffset({
        height: latestHeight + 1,
      });

      expect(nextBlockResponse).toBeSuccess();
      expect(nextBlockResponse.data?.block).toBeNull();

      log.debug(`Verified that no block exists after height ${latestHeight}`);
    });

    /**
     * A block query without parameters responds with the expected schema
     *
     * @when we send a block query without parameters
     * @then Indexer should respond with a block according to the requested schema
     */
    test('should respond with a block according to the requested schema', async (ctx: TestContext) => {
      ctx.task!.meta.custom = {
        labels: ['Query', 'Block', 'Latest', 'SchemaValidation'],
        testKey: 'PM-17678',
      };

      log.debug('Requesting latest block from indexer');
      const response: BlockResponse = await indexerHttpClient.getLatestBlock();

      log.debug('Checking if we actually received a block');
      expect(response).toBeSuccess();
      expect(response.data?.block).toBeDefined();

      log.debug('Validating block schema');
      const block = BlockSchema.safeParse(response.data?.block);
      expect(
        block.success,
        `Block schema validation failed ${JSON.stringify(block.error, null, 2)}`,
      ).toBe(true);
    });
  });

  describe('a block query by hash', () => {
    /**
     * A block query by hash returns the expected block if that hash exists
     *
     * @given we get the latest block hash
     * @when we send a block query by hash using that hash
     * @then Indexer should respond with the block with that hash
     */
    test('should return the block with that hash, given that block exists', async (ctx: TestContext) => {
      ctx.task!.meta.custom = {
        labels: ['Query', 'Block', 'ByHash'],
        testKey: 'PM-17679',
      };

      // Everything is already checked in getLatestBlockByHash function
      // If the promise resolves, we know that the block exists and the test passes
      await getLatestBlockByHash();
    });

    /**
     * A block query by hash responds with the expected schema
     *
     * @when we send a block query by hash
     * @then Indexer should respond with a block according to the requested schema
     */
    test('should return blocks according to the requested schema', async (ctx: TestContext) => {
      ctx.task!.meta.custom = {
        labels: ['Query', 'Block', 'ByHash', 'SchemaValidation'],
        testKey: 'PM-17680',
      };

      const blockByHash = await getLatestBlockByHash();

      log.debug('Validating block schema');
      const parsedBlock = BlockSchema.safeParse(blockByHash);
      expect(
        parsedBlock.success,
        `Block schema validation failed ${JSON.stringify(parsedBlock.error, null, 2)}`,
      ).toBe(true);
    });

    /**
     * A block query by hash returns data with a null block if a block with that hash doesn't exist
     *
     * @given we use a hash that doesn't exist on the chain
     * @when we send a block query by hash using that hash
     * @then Indexer should respond with a null block section
     */
    test("should return a null block, given a block with that hash doesn't exist", async (ctx: TestContext) => {
      ctx.task!.meta.custom = {
        labels: ['Query', 'Block', 'ByHash', 'Negative'],
        testKey: 'PM-17681',
      };

      const allZeroHash = '0000000000000000000000000000000000000000000000000000000000000000';
      log.debug(`Requesting a block with hash ${allZeroHash}`);

      const blockByHashResponse = await indexerHttpClient.getBlockByOffset({ hash: allZeroHash });

      expect(blockByHashResponse).toBeSuccess();
      expect(blockByHashResponse.data?.block).toBeNull();
    });

    /**
     * A block query by hash with invalid hashreturns an error
     *
     * @given we fabricate invalid hashes (malformed)
     * @when we send a block query by hash using them
     * @then Indexer should respond with an error
     */
    test('should return an error, when the hash is invalid (malformed)', async (ctx: TestContext) => {
      ctx.task!.meta.custom = {
        labels: ['Query', 'Block', 'ByHash', 'Negative'],
        testKey: 'PM-17683',
      };

      const fabricatedMalformedHashes = dataProvider.getFabricatedMalformedHashes();

      for (const targetHash of fabricatedMalformedHashes) {
        log.debug(`Requesting a block with malformed hash: ${targetHash}`);

        const blockByHashResponse = await indexerHttpClient.getBlockByOffset({ hash: targetHash });

        expect.soft(blockByHashResponse).toBeError();
      }
    });
  });

  describe('a block query by height', () => {
    /**
     * A block query by height returns the expected block if that height exists
     *
     * @given we use the height of the latest block
     * @when we send a block query by height using that height
     * @then Indexer should respond with the block with that height
     */
    test('should return the block with that height, given a valid height', async (ctx: TestContext) => {
      ctx.task!.meta.custom = {
        labels: ['Query', 'Block', 'ByHeight'],
        testKey: 'PM-17339',
      };

      // Everything is already checked in getLatestBlockByHeight function
      // If the promise resolves, we know that the block exists and the test passes
      await getLatestBlockByHeight();
    });

    /**
     * A block query by height responds with the expected schema
     *
     * @when we send a block query by height
     * @then Indexer should respond with a block according to the requested schema
     */
    test('should return a blocks according to the requested schema', async (ctx: TestContext) => {
      ctx.task!.meta.custom = {
        labels: ['Query', 'Block', 'ByHeight', 'SchemaValidation'],
        testKey: 'PM-17684',
      };

      // Everything is already checked in getLatestBlockByHeight function
      // If the promise resolves, we know that the block exists and the test passes
      const blockByHeight = await getLatestBlockByHeight();

      log.debug('Validating block schema');
      const parsedBlock = BlockSchema.safeParse(blockByHeight);
      expect(
        parsedBlock.success,
        `Block schema validation failed ${JSON.stringify(parsedBlock.error, null, 2)}`,
      ).toBe(true);
    });

    /**
     * A block query by height = 0 returns genesis block
     *
     * @given we use a height = 0
     * @when we send a block query by height using that height
     * @then Indexer should respond with the genesis block
     */
    test('should return the genesis block, given height=0 is requested', async (ctx: TestContext) => {
      ctx.task!.meta.custom = {
        labels: ['Query', 'Block', 'ByHeight', 'Genesis'],
        testKey: 'PM-17685',
      };

      log.debug(`Requesting genesis block (height = 0)`);

      const queryResponse = await indexerHttpClient.getBlockByOffset({ height: 0 });

      expect(
        queryResponse.errors,
        `Received unexpected error ${JSON.stringify(queryResponse.errors, null, 2)}`,
      ).toBeUndefined();
      expect(queryResponse).toBeSuccess();
      expect(queryResponse.data?.block).toBeDefined();
      expect(queryResponse.data?.block.height).toBe(0);
      expect(queryResponse.data?.block.parent).toBeNull();
    });

    /**
     * A block query by height with a height that doesn't exist (but it's within the reange of possible values)
     * returns a null block. Note that this is different from a block query by height with an invalid
     * height, which returns an error. We will use the maximum allowed height for this test.
     *
     * @given we use a height that doesn't exist
     * @when we send a block query by height using that height
     * @then Indexer should respond with an empty (null)block
     */
    test('should return an empty body answer, given that block height requested is the maximum available height', async (ctx: TestContext) => {
      ctx.task!.meta.custom = {
        labels: ['Query', 'Block', 'ByHeight', 'Negative'],
        testKey: 'PM-17686',
      };

      const maxAllowedBlockHeight = 2 ** 32 - 1; // Note this is the maximum allowed height and will take 800+ years to reach
      log.debug(`Requesting block with max height = ${maxAllowedBlockHeight}`);

      const queryResponse = await indexerHttpClient.getBlockByOffset({
        height: maxAllowedBlockHeight,
      });

      expect(queryResponse).toBeSuccess();
      expect(queryResponse.data?.block).toBeDefined();
      expect(queryResponse.data?.block).toBeNull();
    });

    /**
     * A block query by height with an invalid height returns an error
     *
     * @given we fabricate invalid heights
     * @when we send a block query by height using them
     * @then Indexer should respond with an error
     */
    test('should return an error, given an invalid height', async (ctx: TestContext) => {
      ctx.task!.meta.custom = {
        labels: ['Query', 'Block', 'ByHeight', 'Negative'],
        testKey: 'PM-17687',
      };

      const invalidHeights = dataProvider.getFabricatedMalformedHeights();

      for (const targetHeight of invalidHeights) {
        log.debug(`Requesting block with height = ${targetHeight}`);

        const queryResponse = await indexerHttpClient.getBlockByOffset({
          height: targetHeight,
        });

        expect.soft(queryResponse).toBeError();
      }
    });
  });

  describe('a block query by height and hash', () => {
    /**
     * A block query by height and hash returns an error as the indexer only supports one parameter at a time
     * regardless of the validity of the parameters
     *
     * @given we use both height and hash
     * @when we send a block query with both parameters
     * @then Indexer should respond with an error
     */
    test('should return an error, as only one parameter at a time can be used', async (ctx: TestContext) => {
      ctx.task!.meta.custom = {
        labels: ['Query', 'Block', 'ByHeightAndHash', 'Negative'],
        testKey: 'PM-17688',
      };

      // Here we cover the 4 combinations of valid and invalid parameters (hash and height)
      const hashes = ['0'.repeat(64), 'invalid-hash'];
      const heights = [1, 2 ** 32];

      // Generate cartesian product of hashes and heights
      const inputParameters = hashes.flatMap((hash) => heights.map((height) => ({ hash, height })));

      for (const inputParameter of inputParameters) {
        const queryResponse = await indexerHttpClient.getBlockByOffset(inputParameter);
        expect.soft(queryResponse).toBeError();
      }
    });
  });

  /**
   * Coverage for midnight-indexer#1139.
   *
   * Three new fields were added to the `Block` GraphQL type:
   *   - `zswapEndIndex: Int!`
   *   - `dustCommitmentEndIndex: Int! @beta`
   *   - `dustGenerationEndIndex: Int! @beta`
   *
   * These carry the chain's per-tree first-free index as of each block,
   * monotonically non-decreasing, letting wallets bound their sync range
   * directly from a block query instead of scanning transactions.
   */
  describe('block-level tree end indexes', () => {
    /**
     * @given the latest indexed block
     * @when tree end index fields are read
     * @then zswapEndIndex, dustCommitmentEndIndex and dustGenerationEndIndex
     *       are non-negative integers
     *
     * midnight-indexer#1139
     */
    test('should return non-negative integers for all three tree end index fields', async (ctx: TestContext) => {
      ctx.task!.meta.custom = {
        labels: ['Query', 'Block', 'EndIndex', '#1139'],
      };

      const response = await indexerHttpClient.getLatestBlock();
      expect(response).toBeSuccess();
      const block = response.data!.block;

      log.debug(
        `Latest block ${block.hash} (height=${block.height}): ` +
          `zswapEndIndex=${block.zswapEndIndex}, ` +
          `dustCommitmentEndIndex=${block.dustCommitmentEndIndex}, ` +
          `dustGenerationEndIndex=${block.dustGenerationEndIndex}`,
      );

      expect(block.zswapEndIndex).toBeGreaterThanOrEqual(0);
      expect(Number.isInteger(block.zswapEndIndex)).toBe(true);

      expect(block.dustCommitmentEndIndex).toBeGreaterThanOrEqual(0);
      expect(Number.isInteger(block.dustCommitmentEndIndex)).toBe(true);

      expect(block.dustGenerationEndIndex).toBeGreaterThanOrEqual(0);
      expect(Number.isInteger(block.dustGenerationEndIndex)).toBe(true);
    });

    /**
     * @given two consecutive blocks — the latest block and its parent
     * @when tree end indexes are compared between parent and child
     * @then each index on the parent is <= the same index on the child
     */
    test('should return non-decreasing tree end indexes from parent to child block', async (ctx: TestContext) => {
      ctx.task!.meta.custom = {
        labels: ['Query', 'Block', 'EndIndex', 'Monotonicity', '#1139'],
      };

      const latestResponse = await indexerHttpClient.getLatestBlock();
      expect(latestResponse).toBeSuccess();
      const latest = latestResponse.data!.block;

      if (latest.height === 0) {
        log.warn('Only genesis block available — skipping monotonicity check');
        ctx.skip?.();
        return;
      }

      const parentResponse = await indexerHttpClient.getBlockByOffset({
        height: latest.height - 1,
      });
      expect(parentResponse).toBeSuccess();
      const parent = parentResponse.data!.block;

      log.debug(
        `Monotonicity: parent(h=${parent.height}) zswap=${parent.zswapEndIndex} dustC=${parent.dustCommitmentEndIndex} dustG=${parent.dustGenerationEndIndex}` +
          ` ≤ child(h=${latest.height}) zswap=${latest.zswapEndIndex} dustC=${latest.dustCommitmentEndIndex} dustG=${latest.dustGenerationEndIndex}`,
      );

      expect(latest.zswapEndIndex).toBeGreaterThanOrEqual(parent.zswapEndIndex);
      expect(latest.dustCommitmentEndIndex).toBeGreaterThanOrEqual(parent.dustCommitmentEndIndex);
      expect(latest.dustGenerationEndIndex).toBeGreaterThanOrEqual(parent.dustGenerationEndIndex);
    });
  });

  // Indexers up to 4.3.3 resolve the Block dust root fields at the latest indexed
  // state (the tip) by design, so per-block assertions only hold on deployments
  // that include the per-block change. Each test probes the deployed schema and
  // skips on tip-scoped deployments rather than false-failing there.
  describe('per-block dust merkle tree roots', () => {
    /**
     * Dust roots belong to the queried block, not the tip.
     *
     * @given two blocks between which the dust generation tree has grown
     *        (their dustGenerationEndIndex values differ)
     * @when both blocks' dust generation Merkle tree roots are read
     * @then the roots differ — a tree with more leaves cannot share a root
     *       with its earlier, smaller state
     *
     * midnight-indexer#1260
     */
    test('should return different dust generation roots for blocks with different tree sizes', async (ctx: TestContext) => {
      ctx.task!.meta.custom = {
        labels: ['Query', 'Block', 'Dust', 'MerkleRoot', '#1260'],
      };

      if (!(await isPerBlockDustRootsSupported())) {
        ctx.skip?.(true, 'deployed indexer serves tip-scoped dust roots (pre per-block change)');
        return;
      }

      const latestResponse = await indexerHttpClient.getLatestBlock();
      expect(latestResponse).toBeSuccess();
      const tip = latestResponse.data!.block;

      const earlierResponse = await indexerHttpClient.getBlockByOffset({
        height: Math.floor(tip.height / 2),
      });
      expect(earlierResponse).toBeSuccess();
      const earlier = earlierResponse.data!.block;

      if (earlier.dustGenerationMerkleTreeRoot === null) {
        ctx.skip?.(
          true,
          `block ${earlier.height} has no stored dust roots — history predates the per-block change`,
        );
        return;
      }

      if (earlier.dustGenerationEndIndex === tip.dustGenerationEndIndex) {
        ctx.skip?.(
          true,
          `generation tree did not grow between block ${earlier.height} and block ${tip.height} ` +
            `(endIndex ${tip.dustGenerationEndIndex}) — root comparison is vacuous`,
        );
        return;
      }

      log.debug(
        `Block ${earlier.height} (endIndex=${earlier.dustGenerationEndIndex}) root=${earlier.dustGenerationMerkleTreeRoot} vs ` +
          `block ${tip.height} (endIndex=${tip.dustGenerationEndIndex}) root=${tip.dustGenerationMerkleTreeRoot}`,
      );

      expect(tip.dustGenerationMerkleTreeRoot).not.toBeNull();
      expect(earlier.dustGenerationMerkleTreeRoot).not.toBe(tip.dustGenerationMerkleTreeRoot);

      if (earlier.dustCommitmentEndIndex !== tip.dustCommitmentEndIndex) {
        expect(tip.dustCommitmentMerkleTreeRoot).not.toBeNull();
        expect(earlier.dustCommitmentMerkleTreeRoot).not.toBe(tip.dustCommitmentMerkleTreeRoot);
      }
    });

    /**
     * Per-block dust roots are stable: the same block always reports the same roots.
     *
     * @given a historical block queried twice by height
     * @when the dust commitment and generation roots of both responses are compared
     * @then both queries return identical, non-null roots
     *
     * midnight-indexer#1260
     */
    test('should return identical dust roots for repeated queries of the same block', async (ctx: TestContext) => {
      ctx.task!.meta.custom = {
        labels: ['Query', 'Block', 'Dust', 'MerkleRoot', '#1260'],
      };

      if (!(await isPerBlockDustRootsSupported())) {
        ctx.skip?.(true, 'deployed indexer serves tip-scoped dust roots (pre per-block change)');
        return;
      }

      const latestResponse = await indexerHttpClient.getLatestBlock();
      expect(latestResponse).toBeSuccess();
      const height = Math.floor(latestResponse.data!.block.height / 2);

      const firstResponse = await indexerHttpClient.getBlockByOffset({ height });
      expect(firstResponse).toBeSuccess();
      const first = firstResponse.data!.block;

      if (first.dustGenerationMerkleTreeRoot === null) {
        ctx.skip?.(
          true,
          `block ${height} has no stored dust roots — history predates the per-block change`,
        );
        return;
      }

      const secondResponse = await indexerHttpClient.getBlockByOffset({ height });
      expect(secondResponse).toBeSuccess();
      const second = secondResponse.data!.block;

      expect(second.dustGenerationMerkleTreeRoot).toBe(first.dustGenerationMerkleTreeRoot);
      expect(second.dustCommitmentMerkleTreeRoot).toBe(first.dustCommitmentMerkleTreeRoot);
      expect(first.dustCommitmentMerkleTreeRoot).not.toBeNull();
    });

    /**
     * Root and tree size move together: an unchanged tree keeps its root.
     *
     * @given the latest block and its parent
     * @when their dust generation end indexes are equal
     * @then their dust generation Merkle tree roots are equal as well
     *       (same leaves, same tree, same root)
     *
     * midnight-indexer#1260
     */
    test('should return equal dust generation roots for parent and child with equal tree sizes', async (ctx: TestContext) => {
      ctx.task!.meta.custom = {
        labels: ['Query', 'Block', 'Dust', 'MerkleRoot', '#1260'],
      };

      if (!(await isPerBlockDustRootsSupported())) {
        ctx.skip?.(true, 'deployed indexer serves tip-scoped dust roots (pre per-block change)');
        return;
      }

      const latestResponse = await indexerHttpClient.getLatestBlock();
      expect(latestResponse).toBeSuccess();
      const latest = latestResponse.data!.block;

      if (latest.height === 0) {
        ctx.skip?.(true, 'only genesis block available — no parent to compare against');
        return;
      }

      const parentResponse = await indexerHttpClient.getBlockByOffset({
        height: latest.height - 1,
      });
      expect(parentResponse).toBeSuccess();
      const parent = parentResponse.data!.block;

      if (parent.dustGenerationEndIndex !== latest.dustGenerationEndIndex) {
        ctx.skip?.(
          true,
          `generation tree grew between block ${parent.height} and block ${latest.height} — ` +
            'equal-size comparison is not applicable',
        );
        return;
      }

      expect(latest.dustGenerationMerkleTreeRoot).not.toBeNull();
      expect(latest.dustGenerationMerkleTreeRoot).toBe(parent.dustGenerationMerkleTreeRoot);
    });
  });
});

/**
 * Extracts and returns all the transactions from the genesis block.
 *
 * @param block - The genesis block object to extract the transactions from.
 * @returns The array of Transaction objects contained in the genesis block.
 */
async function extractGenesisTransactions(block: Block): Promise<Transaction[]> {
  expect(block.transactions).toBeDefined();
  expect(block.transactions).not.toBeNull();
  expect(block.transactions.length).toBeGreaterThanOrEqual(1);

  return block.transactions as Transaction[];
}

describe(`genesis block`, () => {
  let genesisBlock: Block;

  beforeEach(async () => {
    const blockQueryResponse: BlockResponse = await indexerHttpClient.getBlockByOffset({
      height: 0,
    });
    expect(blockQueryResponse).toBeSuccess();
    expect(blockQueryResponse.data?.block).toBeDefined();

    genesisBlock = blockQueryResponse.data?.block as Block;
    expect(genesisBlock.ledgerParameters).toBeTruthy();
  });

  describe(`a block query to the genesis block`, async () => {
    /**
     * Genesis block contains transactions with pre-fund wallet utxos
     *
     * @given the genesis block is queried
     * @when we inspect its transactions
     * @then it should contain transactions with pre-fund wallet utxos
     */
    test('should contain transactions with pre-fund wallet utxos', async (ctx: TestContext) => {
      ctx.task!.meta.custom = {
        labels: ['Query', 'Block', 'ByHeight', 'Genesis', 'PreFundWallets', 'UnshieldedTokens'],
        testKey: 'PM-17689',
      };

      const genesisTransactions = await extractGenesisTransactions(genesisBlock);
      expect(genesisTransactions).toBeDefined();
      expect(genesisTransactions.length).toBeGreaterThanOrEqual(1);

      for (const transaction of genesisTransactions) {
        if (transaction.__typename === 'RegularTransaction') {
          const regularTransaction = transaction as RegularTransaction;
          if (regularTransaction.identifiers?.length === 1) {
            expect(regularTransaction.unshieldedCreatedOutputs).toBeDefined();
            expect(regularTransaction.unshieldedCreatedOutputs?.length).toBeGreaterThanOrEqual(1);
          } else {
            expect(regularTransaction.raw).toBeDefined();
            expect(regularTransaction.raw).not.toBeNull();
          }
        }
      }
    });

    /**
     * Genesis block contains utxos related to exactly 4 pre-fund wallets
     *
     * @given the genesis block is queried
     * @when we inspect the utxos in its transaction
     * @then there should be utxos related to exactly 4 pre-fund wallets
     */
    test('should contain utxos related to exactly 4 pre-fund wallets', async (ctx: TestContext) => {
      ctx.task!.meta.custom = {
        labels: ['Query', 'Block', 'ByHeight', 'Genesis', 'PreFundWallets', 'UnshieldedTokens'],
        testKey: 'PM-17690',
      };

      const expectedPreFundWallets = 4;
      const genesisTransactions = await extractGenesisTransactions(genesisBlock);

      // Loop through all the utxos in the transactions that have them and gather all
      // the pre-fund wallet addresses
      const preFundWallets: Set<string> = new Set();
      for (const transaction of genesisTransactions) {
        if (transaction.__typename === 'RegularTransaction') {
          const regularTransaction = transaction as RegularTransaction;
          const utxos = regularTransaction.unshieldedCreatedOutputs;
          if (utxos!.length > 0) {
            for (const utxo of utxos!) {
              preFundWallets.add(utxo.owner);
              log.debug(`pre-fund wallet found: ${utxo.owner}`);
            }
          }
        }
      }

      expect(preFundWallets).toHaveLength(expectedPreFundWallets);
    });

    /**
     * Genesis block contains utxos with exactly 1 token type
     *
     * @given the genesis block is queried
     * @when we inspect the utxos in its transaction
     * @then there should be utxos with exactly 1 token type
     */
    test('should contain utxos with exactly 1 token type', async (ctx: TestContext) => {
      ctx.task!.meta.custom = {
        labels: ['Query', 'Block', 'ByHeight', 'Genesis', 'PreFundWallets', 'UnshieldedTokens'],
        testKey: 'PM-17691',
      };

      const expectedTokenTypes = 1;
      const genesisTransactions = await extractGenesisTransactions(genesisBlock);

      // Loop through all the utxos in the transactions that have them and gather all
      // the token types
      const tokenTypes: Set<string> = new Set();
      for (const transaction of genesisTransactions) {
        if (transaction.__typename === 'RegularTransaction') {
          const regularTransaction = transaction as RegularTransaction;
          const utxos = regularTransaction.unshieldedCreatedOutputs;
          if (utxos!.length > 0) {
            for (const utxo of utxos!) {
              tokenTypes.add(utxo.tokenType);
              log.debug(`tokenType found: ${utxo.tokenType}`);
            }
          }
        }
      }

      expect(tokenTypes).toHaveLength(expectedTokenTypes);
    });

    /**
     * Genesis block contains utxos sorted by outputIndex in ascending order
     *
     * @given the genesis block is queried
     * @when we inspect the utxos in its transaction
     * @then the utxos should be sorted by outputIndex in ascending order
     */
    // https://shielded.atlassian.net/browse/PM-17665
    test('should contain utxos sorted by outputIndex in ascending order', async (ctx: TestContext) => {
      ctx.task!.meta.custom = {
        labels: ['Query', 'Block', 'ByHeight', 'Genesis', 'PreFundWallets', 'UnshieldedTokens'],
        testKey: 'PM-17692',
      };

      const genesisTransactions = await extractGenesisTransactions(genesisBlock);

      // Loop through all the utxos in the transactions that have them and gather all
      // the output indexes
      const outputIndexes: Set<number> = new Set();
      for (const transaction of genesisTransactions) {
        if (transaction.__typename === 'RegularTransaction') {
          const regularTransaction = transaction as RegularTransaction;
          const utxos = regularTransaction.unshieldedCreatedOutputs;
          if (utxos!.length > 0) {
            for (const utxo of utxos!) {
              outputIndexes.add(utxo.outputIndex);
              log.debug(`outputIndex found: ${utxo.outputIndex}`);
            }
          }
        }
      }

      expect(outputIndexes).toBeDefined();
      expect(outputIndexes).not.toBeNull();
      expect(outputIndexes.size).toBeGreaterThanOrEqual(1);
      const utxos = Array.from(outputIndexes) as number[];

      // Loop through all the utxos in the genesis transaction and check whether the
      // they are sorted by outputIndex in ascending order
      let previousOutputIndex = utxos[0];
      let currentOutputIndex: number;
      for (let i = 1; i < utxos.length; i++) {
        currentOutputIndex = utxos[i];

        // NOTE: We don't need to check that outputIndex values are strictly sequential (e.g., 0, 1, 2, ... N);
        // we only need to verify that they are sorted in ascending order.
        log.debug(
          `previousOutputIndex = ${previousOutputIndex} currentOutputIndex = ${currentOutputIndex}`,
        );
        expect.soft(currentOutputIndex).toBeGreaterThan(previousOutputIndex);
      }
    });

    /**
     * Genesis block regular transactions should have valid index ranges
     *
     * @given the genesis block is indexed
     * @when we inspect its regular transactions
     * @then endIndex >= startIndex for zswap, dustCommitment, and dustGeneration indices
     */
    test('should contain valid index ranges on regular transactions', async (ctx: TestContext) => {
      ctx.task!.meta.custom = {
        labels: ['Query', 'Transaction', 'IndexFields', 'Genesis'],
      };

      const genesisTransactions = await extractGenesisTransactions(genesisBlock);
      const regularTxs = genesisTransactions.filter(
        (tx) => tx.__typename === 'RegularTransaction',
      ) as RegularTransaction[];

      expect(regularTxs.length).toBeGreaterThan(0);

      for (const tx of regularTxs) {
        log.debug(
          `Transaction ${tx.hash}: zswap=[${tx.zswapStartIndex}, ${tx.zswapEndIndex}], dustCommitment=[${tx.dustCommitmentStartIndex}, ${tx.dustCommitmentEndIndex}], dustGeneration=[${tx.dustGenerationStartIndex}, ${tx.dustGenerationEndIndex}]`,
        );

        expect(tx.zswapEndIndex!).toBeGreaterThanOrEqual(tx.zswapStartIndex!);
        expect(tx.dustCommitmentEndIndex!).toBeGreaterThanOrEqual(tx.dustCommitmentStartIndex!);
        expect(tx.dustGenerationEndIndex!).toBeGreaterThanOrEqual(tx.dustGenerationStartIndex!);
      }
    });

    /**
     * @given the genesis block containing RegularTransactions and a SystemTransaction
     * @when block-level end indexes are compared to the max RegularTransaction end indexes
     * @then each block-level end index is >= the RegularTransaction max
     *       (the SystemTransaction seeding dust generation advances the block
     *       index beyond what RegularTransactions alone contribute,
     *       e.g. dustGenerationEndIndex 85 vs RegularTransaction max 20)
     *
     * midnight-indexer#1139
     */
    test('should return block-level end indexes >= max RegularTransaction end indexes', async (ctx: TestContext) => {
      ctx.task!.meta.custom = {
        labels: ['Query', 'Block', 'EndIndex', 'Consistency', '#1139'],
      };

      const genesisTransactions = await extractGenesisTransactions(genesisBlock);
      const regularTxs = genesisTransactions.filter(
        (tx) => tx.__typename === 'RegularTransaction',
      ) as RegularTransaction[];

      expect(regularTxs.length).toBeGreaterThan(0);

      const maxZswap = Math.max(...regularTxs.map((tx) => tx.zswapEndIndex ?? 0));
      const maxDustCommitment = Math.max(...regularTxs.map((tx) => tx.dustCommitmentEndIndex ?? 0));
      const maxDustGeneration = Math.max(...regularTxs.map((tx) => tx.dustGenerationEndIndex ?? 0));

      log.debug(
        `Genesis block end indexes — block: zswap=${genesisBlock.zswapEndIndex}, ` +
          `dustC=${genesisBlock.dustCommitmentEndIndex}, dustG=${genesisBlock.dustGenerationEndIndex} | ` +
          `max tx: zswap=${maxZswap}, dustC=${maxDustCommitment}, dustG=${maxDustGeneration}`,
      );

      expect(genesisBlock.zswapEndIndex).toBeGreaterThanOrEqual(maxZswap);
      expect(genesisBlock.dustCommitmentEndIndex).toBeGreaterThanOrEqual(maxDustCommitment);
      expect(genesisBlock.dustGenerationEndIndex).toBeGreaterThanOrEqual(maxDustGeneration);
    });
  });
});
