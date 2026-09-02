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
import { GraphQLClient } from 'graphql-request';
import { retry } from '@utils/retry-helper';
import type {
  Block,
  BlockOffset,
  BlockResponse,
  GraphQLResponse,
  Transaction,
  TransactionOffset,
  TransactionResponse,
  ContractAction,
  ContractActionOffset,
  ContractActionResponse,
  DustGenerationStatus,
  DustGenerationStatusResponse,
  DustGenerations,
  DustGenerationsResponse,
  DustCommitmentMerkleTreeUpdateResult,
  DustCommitmentMerkleTreeUpdateResponse,
  DustGenerationMerkleTreeUpdateResult,
  DustGenerationMerkleTreeUpdateResponse,
  ZswapMerkleTreeCollapsedUpdateResponse,
  ZswapMerkleTreeCollapsedUpdateResult,
  ContractEvent,
  ContractEventFilter,
  ContractEventResponse,
  BridgeEvent,
  BridgeEventsResponse,
  BridgeDepositsResponse,
  BridgeBalanceResponse,
  BridgePoolSummaryResponse,
  BridgeReserveInflowsResponse,
  BridgeTreasuryInflowsResponse,
  BridgeTreasuryReason,
  BlockContractZswapStateResponse,
  ExecutionInputsResponse,
  ContractType,
  ContractActionTypeEnum,
  ContractResponse,
} from './indexer-types';
import {
  GET_LATEST_BLOCK,
  GET_BLOCK_BY_OFFSET,
  GET_ZSWAP_MERKLE_TREE_COLLAPSED_UPDATE,
  GET_BLOCK_CONTRACT_ZSWAP_STATE,
  GET_EXECUTION_INPUTS,
} from './graphql/block-queries';
import { GET_TRANSACTION_BY_OFFSET } from './graphql/transaction-queries';
import { GET_CONTRACT_EVENTS } from './graphql/contract-event-queries';
import { GET_CONTRACT_ACTION, GET_CONTRACT_ACTION_BY_OFFSET } from './graphql/contract-queries';
import { GET_CONTRACT } from './graphql/contract-type-queries';
import {
  GET_DUST_GENERATION_STATUS,
  GET_DUST_GENERATIONS,
  GET_DUST_COMMITMENT_MERKLE_TREE_UPDATE,
  GET_DUST_GENERATION_MERKLE_TREE_UPDATE,
} from './graphql/dust-queries';
import {
  GET_BRIDGE_EVENTS,
  GET_BRIDGE_BALANCE,
  GET_BRIDGE_DEPOSITS,
} from './graphql/bridge-queries';
import {
  GET_BRIDGE_POOL_SUMMARY,
  GET_BRIDGE_RESERVE_INFLOWS,
  GET_BRIDGE_TREASURY_INFLOWS,
} from './graphql/bridge-pool-queries';

/**
 * Recognise operation-level GraphQL errors that look like *server* failures
 * (vs. legitimate domain errors that negative tests assert on). These are
 * the kinds of failures that are worth retrying — the request was processed
 * but the server failed, and qanet has been observed returning them
 * transiently under load or while re-syncing.
 */
function isTransientServerError(err: { message?: string }): boolean {
  if (typeof err?.message !== 'string') return false;
  return /(internal server error|service unavailable|gateway timeout|panic|deadlock|connection reset|temporarily unavailable)/i.test(
    err.message,
  );
}

/**
 * HTTP client for interacting with the Midnight Indexer GraphQL API
 *
 * This utility class exposes methods to fetch blocks, transactions, and unshielded UTXOs from the indexer.
 * These functions are designed on top of the GraphQL API provided by the indexer so they resemble the
 * GraphQL queries and their parameters.
 *
 * The Graphql query used is hidden from the consumer but it can be overridden passing a custom query to the
 * function.
 *
 * The response is returned as a GraphQLResponse object which contains the data and errors.
 *
 * The response is always logged for debugging purposes.
 *
 */
export class IndexerHttpClient {
  private client: GraphQLClient;
  private readonly graphqlAPIEndpoint: string;
  private targetUrl: string;

  /**
   * Creates a new IndexerHttpClient instance
   * @param endpoint - The base URL for the indexer HTTP endpoint. Defaults to the environment configuration
   */
  constructor() {
    const apiVersion = process.env.INDEXER_API_VERSION?.trim() || 'v4';
    this.graphqlAPIEndpoint = `/api/${apiVersion}/graphql`;
    this.targetUrl = env.getIndexerHttpBaseURL() + this.graphqlAPIEndpoint;
    this.client = new GraphQLClient(this.targetUrl, { errorPolicy: 'all' });
  }

  /**
   * Gets the target URL for GraphQL API requests
   * @returns The complete URL endpoint for GraphQL API calls
   */
  getTargetUrl() {
    return this.targetUrl;
  }

  /**
   * Wraps `client.rawRequest` with retry semantics. graphql-request throws on
   * transport errors (network failures, DNS, ECONN*, TLS) and on non-2xx HTTP
   * responses (e.g. 502/503/504 from the gateway). With `errorPolicy: 'all'`,
   * GraphQL data errors are returned inside the body and NOT thrown.
   *
   * The retry policy is:
   *   - Retry on thrown errors (transport / 5xx / connection-level).
   *   - Retry on HTTP-200 responses whose `errors[]` contain an
   *     operation-level server failure (e.g. "Internal Server Error",
   *     "panic", "timeout"). These are equivalent in spirit to a 5xx —
   *     the request was processed but the server failed on it — and we've
   *     observed qanet returning them transiently under load / sync hiccups.
   *   - Do NOT retry on legitimate GraphQL data errors (e.g. "invalid hash",
   *     "block not found"). Those are what negative tests assert on.
   *
   * Retry budget is intentionally small: it shields against brief upstream
   * blips without masking sustained outages or hiding indexer regressions.
   */
  private rawRequestWithRetry<T>(
    query: string,
    variables?: Record<string, unknown>,
    retryLabel?: string,
  ): Promise<GraphQLResponse<T>> {
    return retry(
      async () => {
        const response = (await this.client.rawRequest<T>(
          query,
          variables,
        )) as unknown as GraphQLResponse<T>;
        if (response.errors && response.errors.some(isTransientServerError)) {
          throw new Error(
            `Transient server-side GraphQL error (will retry): ${JSON.stringify(response.errors)}`,
          );
        }
        return response;
      },
      {
        maxRetries: 2,
        delayMs: 1000,
        retryLabel: retryLabel ?? 'GraphQL HTTP request',
      },
    );
  }

  /**
   * Retrieves the latest block from the indexer
   *
   * @param queryOverride - Optional custom GraphQL query to override the default latest block query
   *
   * @returns Promise resolving to the block response containing the latest block data
   */
  async getLatestBlock(queryOverride?: string): Promise<BlockResponse> {
    log.debug(`Target URL endpoint ${this.getTargetUrl()}`);

    const query = queryOverride || GET_LATEST_BLOCK;
    log.debug(`Using query\n${query}`);

    const response = await this.rawRequestWithRetry<{ block: Block }>(query);

    log.debug(`Raw indexer response\n${JSON.stringify(response, null, 2)}`);

    return response;
  }

  /**
   * Retrieves a specific block by its offset (hash or height) from the indexer
   *
   * @param offset - The block offset to query for
   * @param queryOverride - Optional custom GraphQL query to override the default block query
   *
   * @returns Promise resolving to the block response containing the requested block data
   */
  async getBlockByOffset(offset: BlockOffset, queryOverride?: string): Promise<BlockResponse> {
    log.debug(`Target URL endpoint ${this.getTargetUrl()}`);

    const query = queryOverride || GET_BLOCK_BY_OFFSET;
    const variables = { OFFSET: offset };

    log.debug(`Using query\n${query}`);
    log.debug(`Using variables\n${JSON.stringify(variables, null, 2)}`);

    const response = await this.rawRequestWithRetry<{ block: Block }>(query, variables);

    log.debug(`Raw indexer response\n${JSON.stringify(response, null, 2)}`);

    return response;
  }

  /**
   * Retrieves a transaction by its offset (hash or identifier) from the indexer
   *
   * @param offset - The transaction offset to query for
   * @param queryOverride - Optional custom GraphQL query to override the default transaction query
   *
   * @returns Promise resolving to the transaction response containing the requested transaction data
   */
  async getTransactionByOffset(
    offset: TransactionOffset,
    queryOverride?: string,
  ): Promise<TransactionResponse> {
    log.debug(`Target URL endpoint ${this.getTargetUrl()}`);

    const query = queryOverride || GET_TRANSACTION_BY_OFFSET;
    const variables = { OFFSET: offset };

    log.debug(`Using query\n${query}`);
    log.debug(`Using variables\n${JSON.stringify(variables, null, 2)}`);

    const response = await this.rawRequestWithRetry<{ transactions: Transaction[] }>(
      query,
      variables,
    );

    log.debug(`Raw indexer response\n${JSON.stringify(response, null, 2)}`);

    return response;
  }

  /**
   * Retrieves a contract action by its address and optional offset from the indexer
   *
   * @param contractAddress - The contract address to query for
   * @param offset - The contract action offset to query for (note this could be either a transaction
   *                 offset or a block offset)
   * @param queryOverride - Optional custom GraphQL query to override the default contract action query
   *
   * @returns Promise resolving to the contract action response containing the requested contract action data
   */
  async getContractAction(
    contractAddress: string,
    offset?: ContractActionOffset,
    queryOverride?: string,
  ): Promise<ContractActionResponse> {
    log.debug(`Target URL endpoint ${this.getTargetUrl()}`);

    const query = queryOverride || (offset ? GET_CONTRACT_ACTION_BY_OFFSET : GET_CONTRACT_ACTION);
    const variables = {
      ADDRESS: contractAddress,
      OFFSET: offset,
    };

    log.debug(`Using query\n${query}`);
    log.debug(`Using variables\n${JSON.stringify(variables, null, 2)}`);

    const response = await this.rawRequestWithRetry<{ contractAction: ContractAction }>(
      query,
      variables,
    );

    log.debug(`Raw indexer response\n${JSON.stringify(response, null, 2)}`);

    return response;
  }

  /**
   * Retrieves a zswap Merkle tree collapsed update for the given index range
   *
   * @param startIndex - The start index of the range
   * @param endIndex - The end index of the range
   * @param queryOverride - Optional custom GraphQL query to override the default query
   *
   * @returns Promise resolving to the collapsed update response
   */
  async getZswapMerkleTreeCollapsedUpdate(
    startIndex: number,
    endIndex: number,
    queryOverride?: string,
  ): Promise<ZswapMerkleTreeCollapsedUpdateResponse> {
    log.debug(`Target URL endpoint ${this.getTargetUrl()}`);

    const query = queryOverride || GET_ZSWAP_MERKLE_TREE_COLLAPSED_UPDATE;
    const variables = { START_INDEX: startIndex, END_INDEX: endIndex };

    log.debug(`Using query\n${query}`);
    log.debug(`Using variables\n${JSON.stringify(variables, null, 2)}`);

    const response = await this.rawRequestWithRetry<{
      zswapMerkleTreeCollapsedUpdate: ZswapMerkleTreeCollapsedUpdateResult;
    }>(query, variables);

    log.debug(`Raw indexer response\n${JSON.stringify(response, null, 2)}`);

    return response;
  }

  /**
   * Retrieves DUST generation status for given Cardano reward addresses from the indexer
   * @param cardanoRewardAddresses - Array of Cardano reward addresses to query
   * @param queryOverride - Optional custom GraphQL query to override the default DUST generation status query
   * @returns Promise resolving to the DUST generation status response containing status for each reward address
   */
  async getDustGenerationStatus(
    cardanoRewardAddresses: string[],
    queryOverride?: string,
  ): Promise<DustGenerationStatusResponse> {
    log.debug(`Target URL endpoint ${this.getTargetUrl()}`);

    const query = queryOverride || GET_DUST_GENERATION_STATUS;
    const variables = { CARDANO_REWARD_ADDRESSES: cardanoRewardAddresses };

    log.debug(`Using query\n${query}`);
    log.debug(`Using variables\n${JSON.stringify(variables, null, 2)}`);

    const response = await this.rawRequestWithRetry<{
      dustGenerationStatus: DustGenerationStatus[];
    }>(query, variables);

    log.debug(`Raw indexer response\n${JSON.stringify(response, null, 2)}`);

    return response;
  }

  /**
   * Retrieves all active DUST registrations and aggregated generation stats for given Cardano reward addresses
   * @param cardanoRewardAddresses - Array of Cardano reward addresses to query
   * @param queryOverride - Optional custom GraphQL query
   * @returns Promise resolving to the DUST generations response
   */
  async getDustGenerations(
    cardanoRewardAddresses: string[],
    queryOverride?: string,
  ): Promise<DustGenerationsResponse> {
    log.debug(`Target URL endpoint ${this.getTargetUrl()}`);

    const query = queryOverride || GET_DUST_GENERATIONS;
    const variables = { CARDANO_REWARD_ADDRESSES: cardanoRewardAddresses };

    log.debug(`Using query\n${query}`);
    log.debug(`Using variables\n${JSON.stringify(variables, null, 2)}`);

    const response = await this.rawRequestWithRetry<{
      dustGenerations: DustGenerations[];
    }>(query, variables);

    log.debug(`Raw indexer response\n${JSON.stringify(response, null, 2)}`);

    return response;
  }

  /**
   * Retrieves a collapsed Merkle tree update for the dust commitment tree
   * @param startIndex - Start index of the range
   * @param endIndex - Optional end index of the range
   * @param queryOverride - Optional custom GraphQL query
   * @returns Promise resolving to the hex-encoded collapsed update
   */
  async getDustCommitmentMerkleTreeUpdate(
    startIndex: number,
    endIndex: number,
    queryOverride?: string,
  ): Promise<DustCommitmentMerkleTreeUpdateResponse> {
    log.debug(`Target URL endpoint ${this.getTargetUrl()}`);

    const query = queryOverride || GET_DUST_COMMITMENT_MERKLE_TREE_UPDATE;
    const variables = { START_INDEX: startIndex, END_INDEX: endIndex };

    log.debug(`Using query\n${query}`);
    log.debug(`Using variables\n${JSON.stringify(variables, null, 2)}`);

    const response = await this.rawRequestWithRetry<{
      dustCommitmentMerkleTreeUpdate: DustCommitmentMerkleTreeUpdateResult;
    }>(query, variables);

    log.debug(`Raw indexer response\n${JSON.stringify(response, null, 2)}`);

    return response;
  }

  /**
   * Retrieves a collapsed Merkle tree update for the dust generation tree
   * @param startIndex - Start index of the range
   * @param endIndex - End index of the range (inclusive)
   * @param queryOverride - Optional custom GraphQL query
   * @returns Promise resolving to the hex-encoded collapsed update
   */
  async getDustGenerationMerkleTreeUpdate(
    startIndex: number,
    endIndex: number,
    queryOverride?: string,
  ): Promise<DustGenerationMerkleTreeUpdateResponse> {
    log.debug(`Target URL endpoint ${this.getTargetUrl()}`);

    const query = queryOverride || GET_DUST_GENERATION_MERKLE_TREE_UPDATE;
    const variables = { START_INDEX: startIndex, END_INDEX: endIndex };

    log.debug(`Using query\n${query}`);
    log.debug(`Using variables\n${JSON.stringify(variables, null, 2)}`);

    const response = await this.rawRequestWithRetry<{
      dustGenerationMerkleTreeUpdate: DustGenerationMerkleTreeUpdateResult;
    }>(query, variables);

    log.debug(`Raw indexer response\n${JSON.stringify(response, null, 2)}`);

    return response;
  }

  /**
   * Retrieves public contract events matching a filter from the indexer.
   *
   * @param filter - The contract event filter (contractAddress is required; types,
   *                 fieldPrefixes, fromBlock, toBlock, transactionHash are optional)
   * @param limit - Optional maximum number of events to return
   * @param offset - Optional number of events to skip
   * @param queryOverride - Optional custom GraphQL query to override the default
   *
   * @returns Promise resolving to the contract events response
   */
  async getContractEvents(
    filter: ContractEventFilter,
    limit?: number,
    offset?: number,
    queryOverride?: string,
  ): Promise<ContractEventResponse> {
    log.debug(`Target URL endpoint ${this.getTargetUrl()}`);

    const query = queryOverride || GET_CONTRACT_EVENTS;
    const variables = { FILTER: filter, LIMIT: limit, OFFSET: offset };

    log.debug(`Using query\n${query}`);
    log.debug(`Using variables\n${JSON.stringify(variables, null, 2)}`);

    const response = await this.rawRequestWithRetry<{ contractEvents: ContractEvent[] }>(
      query,
      variables,
    );

    log.debug(`Raw indexer response\n${JSON.stringify(response, null, 2)}`);

    return response;
  }

  async getBridgeEvents(
    filters: {
      recipient?: string;
      variant?: string;
      blockHeightFrom?: number;
      blockHeightTo?: number;
      offset?: number;
      limit?: number;
    } = {},
    queryOverride?: string,
  ): Promise<BridgeEventsResponse> {
    const query = queryOverride || GET_BRIDGE_EVENTS;
    const variables = {
      RECIPIENT: filters.recipient,
      VARIANT: filters.variant,
      BLOCK_HEIGHT_FROM: filters.blockHeightFrom,
      BLOCK_HEIGHT_TO: filters.blockHeightTo,
      OFFSET: filters.offset,
      LIMIT: filters.limit,
    };

    const response = await this.rawRequestWithRetry<{ bridgeEvents: BridgeEvent[] }>(
      query,
      variables,
    );

    log.debug(`Raw indexer response\n${JSON.stringify(response, null, 2)}`);

    return response;
  }

  async getBlockContractZswapState(
    address: string,
    offset?: BlockOffset,
    queryOverride?: string,
  ): Promise<BlockContractZswapStateResponse> {
    const query = queryOverride || GET_BLOCK_CONTRACT_ZSWAP_STATE;
    const variables = { ADDRESS: address, OFFSET: offset };

    const response = await this.rawRequestWithRetry<BlockContractZswapStateResponse['data']>(
      query,
      variables,
    );

    log.debug(`Raw indexer response\n${JSON.stringify(response, null, 2)}`);

    return response;
  }

  async getBridgeBalance(address: string, queryOverride?: string): Promise<BridgeBalanceResponse> {
    const query = queryOverride || GET_BRIDGE_BALANCE;
    const variables = { ADDRESS: address };

    const response = await this.rawRequestWithRetry<BridgeBalanceResponse['data']>(
      query,
      variables,
    );

    log.debug(`Raw indexer response\n${JSON.stringify(response, null, 2)}`);

    return response;
  }

  async getBridgeDeposits(
    recipient: string,
    options: { includeUnapproved?: boolean; offset?: number; limit?: number } = {},
    queryOverride?: string,
  ): Promise<BridgeDepositsResponse> {
    const query = queryOverride || GET_BRIDGE_DEPOSITS;
    const variables = {
      RECIPIENT: recipient,
      INCLUDE_UNAPPROVED: options.includeUnapproved,
      OFFSET: options.offset,
      LIMIT: options.limit,
    };

    const response = await this.rawRequestWithRetry<{ bridgeDeposits: BridgeEvent[] }>(
      query,
      variables,
    );

    log.debug(`Raw indexer response\n${JSON.stringify(response, null, 2)}`);

    return response;
  }

  async getBridgePoolSummary(
    atBlock?: number,
    queryOverride?: string,
  ): Promise<BridgePoolSummaryResponse> {
    const query = queryOverride || GET_BRIDGE_POOL_SUMMARY;
    const variables = { AT_BLOCK: atBlock };

    const response = await this.rawRequestWithRetry<BridgePoolSummaryResponse['data']>(
      query,
      variables,
    );

    log.debug(`Raw indexer response\n${JSON.stringify(response, null, 2)}`);

    return response;
  }

  async getBridgeReserveInflows(
    range: {
      blockHeightFrom?: number;
      blockHeightTo?: number;
      offset?: number;
      limit?: number;
    } = {},
    queryOverride?: string,
  ): Promise<BridgeReserveInflowsResponse> {
    const query = queryOverride || GET_BRIDGE_RESERVE_INFLOWS;
    const variables = {
      BLOCK_HEIGHT_FROM: range.blockHeightFrom,
      BLOCK_HEIGHT_TO: range.blockHeightTo,
      OFFSET: range.offset,
      LIMIT: range.limit,
    };

    const response = await this.rawRequestWithRetry<{ bridgeReserveInflows: BridgeEvent[] }>(
      query,
      variables,
    );

    log.debug(`Raw indexer response\n${JSON.stringify(response, null, 2)}`);

    return response;
  }

  async getBridgeTreasuryInflows(
    options: {
      reason?: BridgeTreasuryReason;
      blockHeightFrom?: number;
      blockHeightTo?: number;
      offset?: number;
      limit?: number;
    } = {},
    queryOverride?: string,
  ): Promise<BridgeTreasuryInflowsResponse> {
    const query = queryOverride || GET_BRIDGE_TREASURY_INFLOWS;
    const variables = {
      REASON: options.reason,
      BLOCK_HEIGHT_FROM: options.blockHeightFrom,
      BLOCK_HEIGHT_TO: options.blockHeightTo,
      OFFSET: options.offset,
      LIMIT: options.limit,
    };

    const response = await this.rawRequestWithRetry<{ bridgeTreasuryInflows: BridgeEvent[] }>(
      query,
      variables,
    );

    log.debug(`Raw indexer response\n${JSON.stringify(response, null, 2)}`);

    return response;
  }

  async getExecutionInputs(
    address: string,
    queryOverride?: string,
  ): Promise<ExecutionInputsResponse> {
    const query = queryOverride || GET_EXECUTION_INPUTS;
    const variables = { ADDRESS: address };

    const response = await this.rawRequestWithRetry<ExecutionInputsResponse['data']>(
      query,
      variables,
    );

    log.debug(`Raw indexer response\n${JSON.stringify(response, null, 2)}`);

    return response;
  }

  async getContract(
    address: string,
    options: {
      offset?: BlockOffset;
      actionsLimit?: number;
      actionsType?: ContractActionTypeEnum;
    } = {},
    queryOverride?: string,
  ): Promise<ContractResponse> {
    const query = queryOverride || GET_CONTRACT;
    const variables = {
      ADDRESS: address,
      OFFSET: options.offset,
      ACTIONS_LIMIT: options.actionsLimit,
      ACTIONS_TYPE: options.actionsType,
    };

    const response = await this.rawRequestWithRetry<{ contract: ContractType | null }>(
      query,
      variables,
    );

    log.debug(`Raw indexer response\n${JSON.stringify(response, null, 2)}`);

    return response;
  }
}
