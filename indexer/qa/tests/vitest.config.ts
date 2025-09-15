import { suite } from '@vitest/runner';
import path from 'path';
import { defineConfig } from 'vitest/config';
import { JUnitReporter } from 'vitest/reporters';
import CustomJUnitReporter from './utils/reporters/custom-junit/custom-junit-reporter';

export default defineConfig({
  test: {
    globals: true,
    environment: 'node',
    globalSetup: path.resolve(__dirname, './utils/logging/setup.ts'),
    setupFiles: [path.resolve(__dirname, './utils/custom-matchers.ts')],
    coverage: {
      reporter: ['text', 'json', 'html'],
    },
    testTimeout: 15000,
    slowTestThreshold: 800,
    retry: 1, // Retry failed tests one extra time just for random glitches
    reporters: [
      'verbose',
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
  },
  resolve: {
    alias: {
      graphql: path.resolve(__dirname, 'node_modules/graphql'),
      '@utils': path.resolve(__dirname, './utils'),
    },
    // This ensures ESM loading doesn't split contexts
    conditions: ['node'],
    mainFields: ['module', 'main'],
  },
  optimizeDeps: {
    include: ['graphql'], // force deduped version
  },
});
