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
import '@utils/logging/test-logging-hooks';
import { IntrospectionQuery } from 'graphql';
import { GraphQLResponse } from '@utils/indexer/indexer-types';
import { IndexerHttpClient } from '@utils/indexer/http-client';
import { IndexerWsClient } from '@utils/indexer/websocket-client';
import {
  SCHEMA_QUERY,
  INTROSPECTION_QUERY,
  BIG_INTROSPECTION_QUERY,
} from '@utils/indexer/graphql/introspection-queries';

describe('graphql health checks', () => {
  describe(`an introspection message sent to the websocket channel`, () => {
    let client: IndexerWsClient;

    beforeEach(async () => {
      client = new IndexerWsClient();
      await client.connectionInit();
    });

    afterEach(async () => {
      await client.connectionClose();
    });

    /**
     * This test verifies that the GraphQL schema introspection works correctly over WebSocket.
     *
     * @Given a WebSocket connection to the GraphQL endpoint
     * @When an introspection query is sent via WebSocket subscription
     * @Then the response should contain the complete GraphQL schema
     */
    test('should return the supported graphql schema', async () => {
      const query = SCHEMA_QUERY;

      const response = await new Promise<GraphQLResponse<IntrospectionQuery>>((resolve, reject) => {
        let dataReceived = false;
        const unsubscribe = client.subscribe(query, {
          next: (data: GraphQLResponse<IntrospectionQuery>) => {
            dataReceived = true;
            unsubscribe();
            resolve(data);
          },
          error: reject,
          complete: () => {
            if (!dataReceived) {
              reject(new Error('No data received before completion'));
            }
          },
        });
      });

      expect(response).toBeSuccess();
      expect(response.data?.__schema).toBeDefined();
      expect(response.data?.__schema?.queryType).toBeDefined();
      expect(response.data?.__schema?.queryType?.name).toBeDefined();
      expect(response.data?.__schema?.queryType?.name).toBe('Query');
    });

    /**
     * This test verifies that the GraphQL endpoint properly handles deep introspection queries
     * by rejecting queries that exceed the maximum recursion depth.
     *
     * @Given a WebSocket connection to the GraphQL endpoint
     * @When an introspection query with depth > 15 is sent via WebSocket subscription
     * @Then the response should contain an error indicating recursion depth limit exceeded
     */
    test('should return an error, given the depth of the query is > 15', async () => {
      const query = BIG_INTROSPECTION_QUERY;

      const response = await new Promise<GraphQLResponse<IntrospectionQuery>>((resolve, reject) => {
        let dataReceived = false;
        const unsubscribe = client.subscribe(query, {
          next: (data: GraphQLResponse<IntrospectionQuery>) => {
            dataReceived = true;
            unsubscribe();
            resolve(data);
          },
          error: reject,
          complete: () => {
            if (!dataReceived) {
              reject(new Error('No data received before completion'));
            }
          },
        });
      });

      log.debug(JSON.stringify(response, null, 2));
      expect(response).toBeError();
    });
  });

  describe(`an introspection request sent to the http channel`, () => {
    let httpClient: IndexerHttpClient;

    beforeEach(async () => {
      httpClient = new IndexerHttpClient();
    });

    /**
     * This test verifies that the GraphQL schema introspection works correctly over HTTP.
     *
     * @Given an HTTP client configured to connect to the GraphQL endpoint
     * @When an introspection query is sent via HTTP POST request
     * @Then the response should contain the complete GraphQL schema
     */
    test('should return the supported graphql schema', async () => {
      const query = INTROSPECTION_QUERY;
      const response: GraphQLResponse<IntrospectionQuery> = await (
        httpClient as any
      ).client.rawRequest(query);

      expect(response).toBeSuccess();
      expect(response.data?.__schema).toBeDefined();
      expect(response.data?.__schema?.queryType).toBeDefined();
      expect(response.data?.__schema?.queryType?.name).toBeDefined();
      expect(response.data?.__schema?.queryType?.name).toBe('Query');
    });

    /**
     * This test verifies that the GraphQL HTTP endpoint properly handles deep introspection queries
     * by rejecting queries that exceed the maximum recursion depth.
     *
     * @Given an HTTP client configured to connect to the GraphQL endpoint
     * @When an introspection query with depth > 15 is sent via HTTP POST request
     * @Then the response should contain an error indicating recursion depth limit exceeded
     */
    test('should return an error, given the depth of the query is > 15', async () => {
      const query = BIG_INTROSPECTION_QUERY;
      const response: GraphQLResponse<IntrospectionQuery> = await (
        httpClient as any
      ).client.rawRequest(query);

      expect(response).toBeError();
      expect(response.errors).toBeDefined();
    });
  });
});
