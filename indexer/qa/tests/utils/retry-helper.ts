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

import log from '@utils/logging/logger';

/**
 * Options for configuring retry behavior
 */
export interface RetryOptions {
  /**
   * Number of EXTRA attempts after the first failure
   * @default 1 (meaning 2 total attempts: initial + 1 retry)
   */
  maxRetries?: number;

  /**
   * Milliseconds to wait between retry attempts
   * @default 1000
   */
  delayMs?: number;

  /**
   * Optional label for better error messages and logging
   */
  retryLabel?: string;
}

/**
 * Retry an async function up to N times if it fails.
 *
 * This function implements a retry pattern (try → wait → retry),
 * but must be async because:
 * 1. JavaScript requires async/await to implement delays (can't block the event loop)
 * 2. The functions being retried are typically async operations (network calls, etc.)
 *
 * Example usage in a beforeAll hook:
 * ```typescript
 * beforeAll(async () => {
 *   await retry(
 *     async () => {
 *       indexerWsClient = new IndexerWsClient();
 *       await indexerWsClient.connectionInit();
 *     },
 *     {
 *       maxRetries: 2,  // will try 3 times total (initial + 2 retries)
 *       delayMs: 2000,  // wait 2 seconds between attempts
 *       retryLabel: 'websocket setup'
 *     }
 *   );
 * });
 * ```
 *
 * Example with partial options (using defaults):
 * ```typescript
 * await retry(
 *   async () => await toolkit.start(),
 *   { retryLabel: 'toolkit start' } // uses default maxRetries=1, delayMs=1000
 * );
 * ```
 *
 * Execution flow:
 * - Attempt 1: Try fn() → if fails, wait delayMs
 * - Attempt 2: Try fn() → if fails, wait delayMs
 * - ... continue until success or maxRetries exhausted
 * - If all attempts fail: throw error with context
 *
 * @param fn - The async function to retry
 * @param options - Configuration options for retry behavior
 * @returns The result of fn() if successful
 * @throws Error with context if all attempts fail
 */
export async function retry<T>(fn: () => Promise<T>, options: RetryOptions = {}): Promise<T> {
  const { maxRetries = 1, delayMs = 1000, retryLabel } = options;

  let lastError: Error | undefined;
  const totalAttempts = maxRetries + 1; // maxRetries=1 means 2 total attempts

  for (let attempt = 1; attempt <= totalAttempts; attempt++) {
    try {
      // Try to execute the function
      return await fn();
    } catch (error) {
      lastError = error as Error;

      // If we have more attempts left, wait before retrying
      if (attempt < totalAttempts) {
        const label = retryLabel ? ` (${retryLabel})` : '';
        const attemptMessage = `Attempt ${attempt}/${totalAttempts} failed ${label}`;
        const errorMessage = `Error: ${error instanceof Error ? error.message : error}`;
        const retryMessage = `Retrying in ${delayMs} ms...`;
        log.warn(`${attemptMessage}\n${errorMessage}\n${retryMessage}`);

        // Wait before the next attempt (this is why the function must be async)
        await new Promise((resolve) => setTimeout(resolve, delayMs));
      }
    }
  }

  // All attempts failed, throw a detailed error
  const label = retryLabel ? ` for ${retryLabel}` : '';
  throw new Error(
    `Failed after ${totalAttempts} attempts${label}. Last error: ${lastError?.message}`,
    { cause: lastError },
  );
}
