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
import '@utils/logging/test-logging-hooks';

describe(`service health checks`, () => {
  const baseUrl = `${env.getIndexerHttpBaseURL()}`;

  describe(`a request to the /ready endpoint`, async () => {
    /**
     * This test checks if the Indexer is ready to serve requests.
     *
     * @When a request is sent to the Indexer's /ready endpoint
     * @Then the response should be OK
     */
    test('should return a 200 status code OK', async () => {
      log.info('Checking Indexer is ready');
      const targetUrl = baseUrl + '/ready';
      log.debug(`Target URL: ${targetUrl}`);
      const response = await fetch(targetUrl);

      expect(response.ok).toBe(true);
    });
  });
});
