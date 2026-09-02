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

import { defineConfig } from 'vitest/config';
import XRayJsonReporter from './utils/reporters/custom-xray-json/xray-json-reporter';
import CustomJUnitReporter from './utils/reporters/custom-junit/custom-junit-reporter';

// Optional cap on Vitest's worker pool. When `VITEST_MAX_WORKERS` is set,
// the value is enforced (positive integer, or "<n>%" percentage); when
// unset, Vitest falls back to its default (all available parallelism).
// Lever for diagnosing load-induced indexer flakiness against shared
// envs (#1135).
function parseMaxWorkers(raw: string | undefined): number | string | undefined {
  if (!raw) return undefined;
  if (raw.endsWith('%')) {
    const n = Number(raw.slice(0, -1));
    if (!Number.isInteger(n) || n < 1) {
      throw new Error(
        `VITEST_MAX_WORKERS must be a positive integer or "<n>%" percentage, got: ${JSON.stringify(raw)}`,
      );
    }
    return raw;
  }
  const n = Number(raw);
  if (!Number.isInteger(n) || n < 1) {
    throw new Error(
      `VITEST_MAX_WORKERS must be a positive integer or "<n>%" percentage, got: ${JSON.stringify(raw)}`,
    );
  }
  return n;
}
const maxWorkers = parseMaxWorkers(process.env.VITEST_MAX_WORKERS);

// Main Vitest configuration - common settings and projects
// - smoke tests: quick health checks, no cache warmup
// - e2e tests: includes global setup for toolkit cache warmup
// - integration tests: runs without cache warmup
// Note: slowTestThreshold is set globally (3000ms) as per-project thresholds don't work in Vitest 3.2.4
export default defineConfig({
  test: {
    ...(maxWorkers !== undefined ? { maxWorkers } : {}),
    // Root-level reporters for all projects
    reporters: [
      'tree',
      new XRayJsonReporter(),
      new CustomJUnitReporter(),
      [
        'junit',
        {
          outputFile: './reports/junit/test-results.xml',
        },
      ],
      [
        'json',
        {
          outputFile: './reports/json/test-results.json',
        },
      ],
    ],
    // Note: slowTestThreshold per-project doesn't work in Vitest 3.2.4
    // Setting a single threshold for all projects
    slowTestThreshold: 3000,
    // Projects reference individual config files
    projects: [
      './vitest.config.smoke.ts',
      './vitest.config.e2e.ts',
      './vitest.config.integration.ts',
    ],
  },
});
