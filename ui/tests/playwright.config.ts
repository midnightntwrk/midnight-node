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
