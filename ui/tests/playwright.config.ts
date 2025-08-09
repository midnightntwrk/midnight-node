// This file is part of midnight-node.
// Copyright (C) 2025 Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
// http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

import type { PlaywrightTestConfig } from '@playwright/test'

const timestamp = new Date().valueOf()

const config: PlaywrightTestConfig = {
  expect: { timeout: 20000 },
  use: {
    ignoreHTTPSErrors: true,
  },
  testIgnore: ['*.js'],
  reporter: [
    ['list'],
    ['junit', { outputFile: `./reports/testResults_${timestamp}.xml` }],
    [
      'allure-playwright',
      {
        resultsDir: './reports/allure-results',
        links: {
          issue: {
            nameTemplate: 'Issue #%s',
            urlTemplate: 'https://shielded.atlassian.net/browse/%s',
          },
          tms: {
            nameTemplate: 'TMS #%s',
            urlTemplate: 'https://shielded.atlassian.net/browse/%s',
          },
          jira: {
            urlTemplate: (v) => `https://shielded.atlassian.net/browse/${v}`,
          },
        },
      },
    ],
  ],
  workers: 1,
  outputDir: './reports/playwrightResults',
  projects: [
    {
      name: 'local',
      testDir: './node/main',
      testIgnore: '*Remote*',
    },
    {
      name: 'fork',
      testDir: './node/fork',
    },
    {
      name: 'remote',
      testDir: './node/main',
      testMatch: '*Remote*',
    },
  ],
}

export default config
