// This file is part of midnightntwrk/midnight-indexer.
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

import path from 'path';
import { defineConfig } from 'vitest/config';

// Integration test configuration - no cache warmup
export default defineConfig({
  test: {
    name: 'integration',
    globals: true,
    environment: 'node',
    setupFiles: [path.resolve(__dirname, './utils/custom-matchers.ts')],
    globalSetup: [
      path.resolve(__dirname, './setup/undeployed-with-data-setup.ts'),
      path.resolve(__dirname, './utils/logging/setup.ts'),
    ],
    coverage: {
      reporter: ['text', 'json', 'html'],
    },
    // 60s per test. Measured against qanet under parallel load: individual
    // HTTP queries can take 20-30s and many tests cluster at the previous
    // 30s budget — they passed only because `retry: 1` re-ran them in a
    // calmer window. 60s gives single-attempt headroom for the slow path
    // and, combined with `retry: 1`, a 120s overall budget per test.
    // Outright sustained outages still surface as failures, just later.
    testTimeout: 60000,
    // Hooks (`beforeEach`/`beforeAll`/etc.) hit the indexer the same way
    // the test bodies do — a slow GraphQL response in a `beforeEach` was
    // exhausting vitest's 10s default hook budget on serial runs against
    // a loaded qanet. Match the test budget so hooks have equivalent
    // headroom and don't fail-by-timeout while the indexer is just slow.
    hookTimeout: 60000,
    retry: 1,
    include: ['tests/integration/**/*.test.ts'],
  },
  resolve: {
    alias: {
      graphql: path.resolve(__dirname, 'node_modules/graphql'),
      '@utils': path.resolve(__dirname, './utils'),
      environment: path.resolve(__dirname, './environment'),
      // Bare, root-relative specifiers (tsconfig `baseUrl: "."`). Vitest 3's
      // bundled Vite resolved these implicitly; Vite 7 (vitest 4) does not,
      // so they must be aliased explicitly.
      utils: path.resolve(__dirname, './utils'),
      tests: path.resolve(__dirname, './tests'),
    },
    conditions: ['node'],
    mainFields: ['module', 'main'],
  },
  optimizeDeps: {
    include: ['graphql'],
  },
});
