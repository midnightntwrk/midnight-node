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

/**
 * Read-only access to the undeployed stack's indexer Postgres via
 * `docker exec … psql`, for tests that need to observe state the GraphQL API
 * does not expose (e.g. the wallet keep-alive heartbeat).
 *
 * Undeployed-only: it requires the docker compose stack of this repository
 * to be running on the local docker daemon.
 */

import { execFile } from 'child_process';
import path from 'path';
import { fileURLToPath } from 'url';
import { promisify } from 'util';

const execFileAsync = promisify(execFile);

const POSTGRES_SERVICE = 'postgres';
const POSTGRES_USER = 'indexer';
const POSTGRES_DB = 'indexer';

let cachedContainerId: string | undefined;

function resolveRepoRoot(): string {
  // This file lives at qa/tests/utils/undeployed/indexer-postgres.ts —
  // four levels up reaches the repo root.
  const here = path.dirname(fileURLToPath(import.meta.url));
  return path.resolve(here, '..', '..', '..', '..');
}

/**
 * Resolve the running indexer Postgres container of the undeployed stack.
 * The compose project name derives from the checkout directory, so the
 * container is looked up through `docker compose ps` instead of a
 * hardcoded name.
 */
async function getPostgresContainerId(): Promise<string> {
  if (!cachedContainerId) {
    const { stdout } = await execFileAsync(
      'docker',
      ['compose', '--profile', 'cloud', 'ps', '-q', POSTGRES_SERVICE],
      { cwd: resolveRepoRoot() },
    );
    const [containerId] = stdout.trim().split('\n');
    if (!containerId) {
      throw new Error('Indexer postgres container not found — is the undeployed stack running?');
    }
    cachedContainerId = containerId;
  }
  return cachedContainerId;
}

async function queryScalar(sql: string): Promise<string> {
  const containerId = await getPostgresContainerId();
  const { stdout } = await execFileAsync('docker', [
    'exec',
    containerId,
    'psql',
    '-U',
    POSTGRES_USER,
    '-d',
    POSTGRES_DB,
    '-t',
    '-A',
    '-c',
    sql,
  ]);
  return stdout.trim();
}

/**
 * Read `wallets.last_active` for the wallet bound to the given API session,
 * as epoch microseconds, or undefined when no wallet row matches the session.
 *
 * `last_active` is written by the `connect` mutation and by the shielded
 * subscription's keep-alive heartbeat, and read by the wallet-indexer to
 * decide which wallets are still actively indexed.
 */
export async function getWalletLastActiveEpochMicros(
  sessionId: string,
): Promise<number | undefined> {
  if (!/^[0-9a-f]+$/.test(sessionId)) {
    throw new Error(`Session ID is not lowercase hex: ${sessionId}`);
  }
  const value = await queryScalar(
    'SELECT (extract(epoch FROM last_active) * 1000000)::bigint ' +
      `FROM wallets WHERE session_id = decode('${sessionId}', 'hex');`,
  );
  return value === '' ? undefined : Number(value);
}
