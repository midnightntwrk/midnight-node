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

// NOTE: This test verifies a transport-level protocol property (HTTP
// Content-Encoding negotiation) rather than domain-level data correctness. It
// sits in the integration suite for lack of a better home, but it belongs to a
// class of "protocol contract" tests — things like WebSocket subprotocol
// negotiation, CORS headers, request size limits, content-type enforcement —
// that do not yet have their own tier. If more tests of this shape accumulate,
// extracting them into a dedicated `contract` (or `protocol`) suite would give
// them a permanent home and keep integration clean for data-level assertions.

import '@utils/logging/test-logging-hooks';
import { probeGraphQLCompression } from '@utils/indexer/http-compression-probe';

describe('graphql http response compression', () => {
  describe('a GraphQL HTTP request with Accept-Encoding: gzip', () => {
    /**
     * @Given a running indexer with HTTP response compression enabled
     * @When a POST request is sent to the GraphQL endpoint with Accept-Encoding: gzip
     * @Then the response status should be 200
     * @And the Content-Encoding header should be gzip
     * @And the response body decompresses and parses as JSON
     */
    test('should return a gzip-compressed response', async () => {
      const result = await probeGraphQLCompression('gzip');

      expect(result.status).toBe(200);
      expect(result.contentEncoding).toBe('gzip');
      expect(result.data).toBeDefined();
    });
  });

  describe('a GraphQL HTTP request with Accept-Encoding: br', () => {
    /**
     * @Given a running indexer with HTTP response compression enabled
     * @When a POST request is sent to the GraphQL endpoint with Accept-Encoding: br
     * @Then the response status should be 200
     * @And the Content-Encoding header should be br
     * @And the response body decompresses and parses as JSON
     */
    test('should return a brotli-compressed response', async () => {
      const result = await probeGraphQLCompression('br');

      expect(result.status).toBe(200);
      expect(result.contentEncoding).toBe('br');
      expect(result.data).toBeDefined();
    });
  });

  describe('a GraphQL HTTP request with Accept-Encoding: zstd', () => {
    /**
     * @Given a running indexer with HTTP response compression enabled
     * @When a POST request is sent to the GraphQL endpoint with Accept-Encoding: zstd
     * @Then the response status should be 200
     * @And the Content-Encoding header should be zstd
     * @And the response body decompresses and parses as JSON
     */
    test('should return a zstd-compressed response', async () => {
      const result = await probeGraphQLCompression('zstd');

      expect(result.status).toBe(200);
      expect(result.contentEncoding).toBe('zstd');
      expect(result.data).toBeDefined();
    });
  });

  describe('a GraphQL HTTP request with Accept-Encoding: identity', () => {
    /**
     * @Given a running indexer with HTTP response compression enabled
     * @When a POST request is sent to the GraphQL endpoint with Accept-Encoding: identity
     * @Then the response status should be 200
     * @And no Content-Encoding header should be present
     * @And the response body parses as JSON
     *
     * Note: Accept-Encoding: identity is sent explicitly rather than omitting
     * the header. Node.js's fetch (undici) unconditionally appends its own
     * Accept-Encoding to every outgoing request, so omitting the header in
     * test code does not produce a headerless wire request. Sending `identity`
     * explicitly overrides that behaviour and instructs the server to serve
     * the response uncompressed.
     */
    test('should return an uncompressed response', async () => {
      const result = await probeGraphQLCompression('identity');

      expect(result.status).toBe(200);
      expect(result.contentEncoding).toBeNull();
      expect(result.data).toBeDefined();
    });
  });
});
