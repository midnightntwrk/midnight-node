// This file is part of midnightntwrk/midnight-indexer
// Copyright (C) 2025 Midnight Foundation
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
import type {
  Block,
  BlockOffset,
  BlockResponse,
  Transaction,
  TransactionOffset,
  TransactionResponse,
  UnshieldedUtxo,
} from './indexer-types';
import { GET_LATEST_BLOCK, GET_BLOCK_BY_OFFSET } from './graphql/block-queries';
import { GET_TRANSACTION_BY_OFFSET } from './graphql/transaction-queries';

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
  private readonly graphqlAPIEndpoint: string = '/api/v1/graphql';
  private targetUrl: string;

  /**
   * Creates a new IndexerHttpClient instance
   * @param endpoint - The base URL for the indexer HTTP endpoint. Defaults to the environment configuration
   */
  constructor() {
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
   * Retrieves the latest block from the indexer
   * @param queryOverride - Optional custom GraphQL query to override the default latest block query
   * @returns Promise resolving to the block response containing the latest block data
   */
  async getLatestBlock(queryOverride?: string): Promise<BlockResponse> {
    log.debug(`Target URL endpoint ${this.getTargetUrl()}`);

    const query = queryOverride || GET_LATEST_BLOCK;
    log.debug(`Using query\n${query}`);

    const response = await this.client.rawRequest<{ block: Block }>(query);

    log.debug(`Raw indexer response\n${JSON.stringify(response, null, 2)}`);

    return response;
  }

  /**
   * Retrieves a specific block by its offset from the indexer
   * @param offset - The block offset to query for
   * @param queryOverride - Optional custom GraphQL query to override the default block query
   * @returns Promise resolving to the block response containing the requested block data
   */
  async getBlockByOffset(offset: BlockOffset, queryOverride?: string): Promise<BlockResponse> {
    log.debug(`Target URL endpoint ${this.getTargetUrl()}`);

    const query = queryOverride || GET_BLOCK_BY_OFFSET;
    const variables = { OFFSET: offset };

    log.debug(`Using query\n${query}`);
    log.debug(`Using variables\n${JSON.stringify(variables, null, 2)}`);

    const response = await this.client.rawRequest<{ block: Block }>(query, variables);

    log.debug(`Raw indexer response\n${JSON.stringify(response, null, 2)}`);

    return response;
  }

  /**
   * Retrieves a shielded transaction by its offset from the indexer
   * @param offset - The transaction offset to query for
   * @param queryOverride - Optional custom GraphQL query to override the default transaction query
   * @returns Promise resolving to the transaction response containing the requested transaction data
   */
  async getShieldedTransaction(
    offset: TransactionOffset,
    queryOverride?: string,
  ): Promise<TransactionResponse> {
    log.debug(`Target URL endpoint ${this.getTargetUrl()}`);
    const query = queryOverride || GET_TRANSACTION_BY_OFFSET;
    const variables = { OFFSET: offset };

    log.debug(`Using query\n${query}`);
    log.debug(`Using variables\n${JSON.stringify(variables, null, 2)}`);

    const response = await this.client.rawRequest<{ transactions: Transaction[] }>(
      query,
      variables,
    );

    log.debug(`Raw indexer response\n${JSON.stringify(response, null, 2)}`);

    return response;
  }
}
