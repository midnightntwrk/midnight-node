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

// global-setup.ts
import fs from 'fs';
import path from 'path';
import { ToolkitWrapper } from '../utils/toolkit/toolkit-wrapper';
import { startCacheProgressReporter, CacheProgressReporter } from '../utils/toolkit/toolkit-cache';
import { env } from '../environment/model';
import dataProvider from '../utils/testdata-provider';

let warmupToolkit: ToolkitWrapper | undefined;

function cleanupOrphanedToolkitDirs(): void {
  const root = path.resolve('./.tmp/toolkit');
  if (!fs.existsSync(root)) return;
  const skipped: string[] = [];
  for (const entry of fs.readdirSync(root)) {
    const full = path.join(root, entry);
    try {
      fs.rmSync(full, { recursive: true, force: true });
    } catch {
      skipped.push(entry);
    }
  }
  if (skipped.length > 0) {
    console.warn(
      `[SETUP] ${skipped.length} orphaned toolkit dir(s) could not be removed (root-owned files from a previous run): ${skipped.join(', ')}. ` +
        `Run \`sudo rm -rf .tmp/toolkit\` to clear them.`,
    );
  }
}

const PREWARM_RETRY_DELAY_MS = 5_000;
const PREWARM_MAX_ATTEMPTS = 5;

/**
 * Pre-warms the funding seed's wallet state so tests don't replay the full ledger
 * on the first generateSingleTx call.
 *
 * Mirrors the retry logic in ToolkitWrapper.warmupCache: RPC timeouts are transient
 * and worth retrying; all other errors are unexpected and logged prominently but not
 * thrown (pre-warming is an optimisation, not a hard requirement for test execution).
 */
async function prewarmFundingSeed(toolkit: ToolkitWrapper, seed: string): Promise<void> {
  for (let attempt = 1; attempt <= PREWARM_MAX_ATTEMPTS; attempt++) {
    try {
      await toolkit.getDustBalance(seed);
      console.log('[SETUP] Funding seed wallet state cached');
      return;
    } catch (error) {
      const msg = String(error);
      if (msg.toLowerCase().includes('request timeout')) {
        if (attempt < PREWARM_MAX_ATTEMPTS) {
          console.warn(
            `[SETUP] Funding seed pre-warm interrupted by RPC timeout ` +
              `(attempt ${attempt}/${PREWARM_MAX_ATTEMPTS}), retrying in ${PREWARM_RETRY_DELAY_MS / 1_000}s…`,
          );
          await new Promise((res) => setTimeout(res, PREWARM_RETRY_DELAY_MS));
          continue;
        }
      }
      // Non-retriable error or max retries exhausted — warn visibly but don't abort setup.
      // Tests can still run; the first generateSingleTx will just be slower.
      console.warn(
        `[SETUP] Funding seed wallet pre-warm failed after ${attempt} attempt(s) — ` +
          `tests will proceed but first generateSingleTx may be slow.\n` +
          `  Cause: ${msg.slice(0, 300)}`,
      );
      return;
    }
  }
}

export async function setup() {
  cleanupOrphanedToolkitDirs();
  console.log('[SETUP] Warming up toolkit cache (this may take several minutes)...');

  let reporter: CacheProgressReporter | undefined;
  try {
    const startTime = Date.now();

    console.log('[SETUP] Creating warmup toolkit instance...');
    warmupToolkit = new ToolkitWrapper({});

    console.log('[SETUP] Starting toolkit container...');
    await warmupToolkit.start();

    // Derive the node HTTP RPC URL from the websocket URL so the reporter
    // can show a live percentage (e.g. "fetch progress: 39,485/715,051 (5.5%) blocks complete").
    const nodeRpcUrl = env
      .getNodeWebsocketBaseURL()
      .replace(/^wss:\/\//, 'https://')
      .replace(/^ws:\/\//, 'http://');
    reporter = startCacheProgressReporter(process.env.TARGET_ENV ?? 'cache', nodeRpcUrl);

    console.log('[SETUP] Syncing cache (please wait, this will take time)...');
    await warmupToolkit.warmupCache();

    const fundingSeed = dataProvider.getFundingSeed();
    if (fundingSeed) {
      console.log('[SETUP] Pre-warming funding seed wallet state...');
      await prewarmFundingSeed(warmupToolkit, fundingSeed);
    }

    const duration = ((Date.now() - startTime) / 1000).toFixed(2);
    console.log(`[SETUP] Toolkit cache warmup complete (${duration}s)`);
  } catch (error) {
    console.error('[SETUP] Failed to warmup toolkit cache:', error);
    throw error;
  } finally {
    reporter?.stop();
    if (warmupToolkit) {
      await warmupToolkit.stop();
    }
  }
}

export async function teardown() {
  // no-op
}
