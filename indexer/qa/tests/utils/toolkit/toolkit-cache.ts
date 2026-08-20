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

import { execFile, spawn } from 'child_process';
import { createServer } from 'net';
import fs from 'fs';
import path from 'path';
import { promisify } from 'util';

const execFileAsync = promisify(execFile);

const CONTAINER_NAME = 'toolkit-postgres';
const POSTGRES_IMAGE = 'postgres:16';
const POSTGRES_USER = 'toolkit';
const POSTGRES_PASSWORD = 'toolkit';
const POSTGRES_DB = 'toolkit';
const POSTGRES_INTERNAL_PORT = 5432;
const READY_TIMEOUT_MS = 30_000;
const READY_POLL_INTERVAL_MS = 1_000;

// Resolved relative to the qa/tests working directory (where vitest is invoked).
// Mirrors the convention used by ToolkitWrapper for `.tmp/toolkit`.
const DATA_DIR = path.resolve('.tmp/toolkit-postgres-data');

export interface ToolkitCacheConnection {
  host: string;
  port: number;
  user: string;
  password: string;
  database: string;
  fetchCacheUrl: string;
}

let cached: Promise<ToolkitCacheConnection> | undefined;

/**
 * Ensure a local Postgres container is running for the midnight-toolkit fetch
 * cache (`MN_FETCH_CACHE`) and return its connection details.
 *
 * Idempotent: subsequent calls within the same process reuse the result; a
 * pre-existing `toolkit-postgres` container (possibly started by another
 * test worker) is detected and reused at its currently-bound port.
 */
export async function ensureToolkitCachePostgres(): Promise<ToolkitCacheConnection> {
  if (!cached) {
    cached = bootstrap().catch((err) => {
      cached = undefined;
      throw err;
    });
  }
  return cached;
}

async function bootstrap(): Promise<ToolkitCacheConnection> {
  fs.mkdirSync(DATA_DIR, { recursive: true });

  const port = await ensureContainer();
  await waitForReady();
  await ensureChainNamesTable();

  // The toolkit container runs with --network host on Linux, which means it
  // shares the host network stack. host.docker.internal does not resolve in
  // that mode; 127.0.0.1 reaches the host-bound postgres port directly.
  const fetchCacheUrl = `postgres://${POSTGRES_USER}:${POSTGRES_PASSWORD}@127.0.0.1:${port}/${POSTGRES_DB}`;
  return {
    host: '127.0.0.1',
    port,
    user: POSTGRES_USER,
    password: POSTGRES_PASSWORD,
    database: POSTGRES_DB,
    fetchCacheUrl,
  };
}

async function ensureChainNamesTable(): Promise<void> {
  const deadline = Date.now() + READY_TIMEOUT_MS;
  for (;;) {
    try {
      await execFileAsync('docker', [
        'exec',
        CONTAINER_NAME,
        'psql',
        // TCP, not the unix socket: the entrypoint's first-run temporary
        // server is socket-only, so the socket can reach a server that is
        // about to be restarted.
        '-h',
        '127.0.0.1',
        '-U',
        POSTGRES_USER,
        '-d',
        POSTGRES_DB,
        '-c',
        `CREATE TABLE IF NOT EXISTS chain_names (
           chain_id      BYTEA PRIMARY KEY,
           env_name      TEXT NOT NULL,
           registered_at TIMESTAMPTZ DEFAULT now()
         );`,
      ]);
      return;
    } catch (err) {
      const message = errorMessage(err);
      // IF NOT EXISTS is not concurrency-safe: two workers can both pass the
      // existence check and race the catalog insert; the loser gets a
      // duplicate-key error on a pg_* catalog index. The only statement here
      // is this CREATE TABLE, so any duplicate-key error means the table
      // already exists — which is all we need.
      if (message.includes('duplicate key value violates unique constraint')) {
        return;
      }
      const transient =
        message.includes('the database system is starting up') ||
        message.includes('connection to server');
      if (!transient || Date.now() >= deadline) {
        throw err;
      }
      await sleep(READY_POLL_INTERVAL_MS);
    }
  }
}

async function ensureContainer(): Promise<number> {
  const existing = await inspectContainer();
  if (existing) {
    if (!existing.running) {
      await execFileAsync('docker', ['start', CONTAINER_NAME]);
    }
    const port = existing.port ?? (await inspectContainer())?.port;
    if (!port) {
      throw new Error(`Could not determine host port for existing ${CONTAINER_NAME} container`);
    }
    return port;
  }

  const port = await getFreePort();
  try {
    await execFileAsync('docker', [
      'run',
      '-d',
      '--name',
      CONTAINER_NAME,
      '-p',
      `${port}:${POSTGRES_INTERNAL_PORT}`,
      '-e',
      `POSTGRES_USER=${POSTGRES_USER}`,
      '-e',
      `POSTGRES_PASSWORD=${POSTGRES_PASSWORD}`,
      '-e',
      `POSTGRES_DB=${POSTGRES_DB}`,
      '-v',
      `${DATA_DIR}:/var/lib/postgresql/data`,
      POSTGRES_IMAGE,
    ]);
    console.log(`[CACHE] Started ${CONTAINER_NAME} on host port ${port} (data: ${DATA_DIR})`);
    return port;
  } catch (err) {
    // Race: another worker may have started the container between our
    // inspect and our run. If so, fall back to inspecting the existing one.
    const message = errorMessage(err);
    if (isNameConflict(message)) {
      const raced = await inspectContainer();
      if (raced) {
        if (!raced.running) await execFileAsync('docker', ['start', CONTAINER_NAME]);
        if (!raced.port) {
          throw new Error(`Lost ${CONTAINER_NAME} startup race but could not read its host port`, {
            cause: err,
          });
        }
        console.log(`[CACHE] Adopted ${CONTAINER_NAME} on host port ${raced.port} after race`);
        return raced.port;
      }
    }
    throw err;
  }
}

interface ContainerInfo {
  running: boolean;
  port?: number;
}

async function inspectContainer(): Promise<ContainerInfo | null> {
  try {
    const { stdout } = await execFileAsync('docker', [
      'inspect',
      '--format',
      `{{.State.Running}}|{{with index .NetworkSettings.Ports "${POSTGRES_INTERNAL_PORT}/tcp"}}{{(index . 0).HostPort}}{{end}}`,
      CONTAINER_NAME,
    ]);
    const [runningStr, portStr] = stdout.trim().split('|');
    const port = portStr ? parseInt(portStr, 10) : undefined;
    return {
      running: runningStr === 'true',
      port: Number.isFinite(port) ? port : undefined,
    };
  } catch {
    return null;
  }
}

async function waitForReady(): Promise<void> {
  const deadline = Date.now() + READY_TIMEOUT_MS;
  while (Date.now() < deadline) {
    try {
      await execFileAsync('docker', [
        'exec',
        CONTAINER_NAME,
        'pg_isready',
        // TCP, not the unix socket: on a fresh data dir the postgres image
        // initializes via a temporary socket-only server before restarting
        // for real, and pg_isready against the socket reports that temporary
        // server as ready. Only the final server listens on TCP.
        '-h',
        '127.0.0.1',
        '-U',
        POSTGRES_USER,
        '-d',
        POSTGRES_DB,
      ]);
      return;
    } catch {
      await sleep(READY_POLL_INTERVAL_MS);
    }
  }
  throw new Error(`${CONTAINER_NAME} did not become ready within ${READY_TIMEOUT_MS}ms`);
}

async function getFreePort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const srv = createServer();
    srv.unref();
    srv.on('error', reject);
    srv.listen(0, '127.0.0.1', () => {
      const addr = srv.address();
      if (addr && typeof addr === 'object') {
        const port = addr.port;
        srv.close(() => resolve(port));
      } else {
        srv.close();
        reject(new Error('Failed to allocate a free port'));
      }
    });
  });
}

function sleep(ms: number): Promise<void> {
  return new Promise((res) => setTimeout(res, ms));
}

function errorMessage(err: unknown): string {
  if (err && typeof err === 'object') {
    const e = err as { stderr?: string | Buffer; message?: string };
    return String(e.stderr ?? e.message ?? '');
  }
  return String(err);
}

function isNameConflict(message: string): boolean {
  return (
    message.includes('is already in use by container') ||
    message.includes('Conflict. The container name') ||
    message.includes('already exists')
  );
}

// ---------------------------------------------------------------------------
// Progress reporter
// ---------------------------------------------------------------------------

const PROGRESS_INTERVAL_MS = 10_000;

interface ChainProgress {
  chainId: string; // abbreviated display form, e.g. "0x3c238ca2…"
  chainIdHex: string; // full 64-char hex, used for chain_names registration
  blockCount: number;
  highestBlock: number;
  envName?: string; // populated from chain_names once registered
}

export interface CacheProgressReporter {
  stop: () => void;
}

/**
 * Start a periodic reporter that prints cache sync progress to the console.
 * Polls highest_verified and raw_block_data_v2 every 10 s, showing block
 * counts per chain_id and flagging any stale chains left over from past
 * env resets.
 *
 * @param label      - Label shown in log prefix, e.g. the TARGET_ENV name.
 * @param nodeRpcUrl - HTTP URL of the Substrate node RPC (e.g.
 *                     https://rpc.preview.midnight.network). When provided, the
 *                     reporter fetches the live chain tip and shows a percentage.
 *
 * Returns a handle whose stop() must be called when warmup completes.
 */
export function startCacheProgressReporter(
  label: string = 'cache',
  nodeRpcUrl?: string,
): CacheProgressReporter {
  const prev = new Map<string, number>();
  // Chains already pruned in this process, so we don't re-issue DELETEs every tick.
  const pruned = new Set<string>();
  // `undeployed` provisions a fresh genesis (new chain_id) on every run, so superseded
  // undeployed chains in the shared cache are garbage and safe to reclaim. Persistent
  // networks (qanet, preview, …) have a single stable chain_id, so a second chain under
  // their label means mis-attribution, not a reset — never auto-prune those; surface
  // them for a targeted manual prune instead.
  const autoPrune = label === 'undeployed';

  let chainTip: number | undefined;
  // Hash of block height 1 on the live node. In the toolkit fetch cache the chain_id *is*
  // the block-1 hash (verified against the qanet/preview indexers), NOT the genesis hash:
  // block 1 incorporates its own content (timestamp/author) on top of genesis, so a chain
  // reset from the SAME genesis still yields a different block-1 hash → a new chain_id.
  // Genesis hash would collide across resets and mis-match a freshly-synced chain to an old
  // one. This is the authoritative identity of the chain THIS run is fetching — far more
  // reliable than guessing from block counts or growth, which let a larger leftover chain
  // win the "active" race (e.g. 601 cached blocks vs a tip of 7 → 8585%).
  let currentChainIdHex: string | undefined;
  let registeredCurrent = false;

  const rpc = async <T>(method: string, params: unknown[]): Promise<T | undefined> => {
    if (!nodeRpcUrl) return undefined;
    try {
      const res = await fetch(nodeRpcUrl, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ id: 1, jsonrpc: '2.0', method, params }),
      });
      const json = (await res.json()) as { result?: T };
      return json?.result;
    } catch {
      return undefined; // Non-fatal — the reporter is best-effort.
    }
  };

  const fetchChainTip = async (): Promise<void> => {
    const header = await rpc<{ number?: string }>('chain_getHeader', []);
    if (header?.number) chainTip = parseInt(header.number, 16);
  };

  const fetchCurrentChainId = async (): Promise<void> => {
    if (currentChainIdHex) return; // block-1 hash is stable for the life of the run
    // Block height 1 (not 0): see the currentChainIdHex comment. Returns null until the
    // node has authored block 1 — harmless, we just retry on the next tick. The 30s
    // genesis settle in the undeployed provisioner ensures block 1 exists well before.
    const hash = await rpc<string>('chain_getBlockHash', [1]);
    const hex = hash?.replace(/^0x/, '').toLowerCase();
    if (hex && /^[0-9a-f]+$/.test(hex)) currentChainIdHex = hex;
  };

  const tick = async () => {
    try {
      await Promise.all([fetchChainTip(), fetchCurrentChainId()]);
      const chains = await queryChainProgress();
      if (chains.length === 0) {
        console.log(`[CACHE:${label}] Waiting for first blocks…`);
        return;
      }

      // Per-interval deltas, used only for the Δ display and the active-chain fallback.
      const deltas = new Map<string, number>();
      for (const c of chains) {
        const seen = prev.has(c.chainId);
        deltas.set(c.chainId, seen ? c.blockCount - (prev.get(c.chainId) ?? 0) : 0);
        prev.set(c.chainId, c.blockCount);
      }

      // The chain THIS run is fetching, identified by its block-1 hash when available.
      const current = currentChainIdHex
        ? chains.find((c) => c.chainIdHex === currentChainIdHex)
        : undefined;

      // Register the current chain to this env deterministically — no need to watch it
      // grow, which a fast (few-second) undeployed warmup never gives us time to observe.
      if (current && !registeredCurrent && current.envName !== label) {
        await registerChainName(current.chainIdHex, label).catch(() => undefined);
        registeredCurrent = true;
        current.envName = label;
      }

      // Chains relevant to this env: the current chain plus same-label / untagged ones.
      const candidates = chains.filter((c) => !c.envName || c.envName === label);
      if (!current && candidates.length === 0) {
        console.log(`[CACHE:${label}] Waiting for first blocks…`);
        return;
      }

      // Active chain: the block-1-hash-matched current chain when known; otherwise fall
      // back to a currently-growing chain, then the highest block count (display guess).
      const growing = candidates.filter((c) => (deltas.get(c.chainId) ?? 0) > 0);
      const activeChainId =
        current?.chainId ??
        (growing.length > 0
          ? growing[0].chainId
          : candidates.reduce((a, b) => (a.blockCount >= b.blockCount ? a : b)).chainId);

      // ---- display ----
      const denom = chainTip !== undefined ? chainTip + 1 : undefined; // heights are 0-indexed
      for (const c of chains) {
        // Skip foreign-env chains (a different label) — noise during this run.
        if (c.envName && c.envName !== label && c.chainId !== activeChainId) continue;
        const delta = deltas.get(c.chainId) ?? 0;
        const isActive = c.chainId === activeChainId;
        const tag = isActive
          ? ''
          : c.envName === label
            ? ' ⚠ stale'
            : ' ↪ unattributed (another env?)';
        // A percentage against the live tip is only meaningful for the current chain;
        // showing it for an unrelated leftover yields nonsense like 8585%.
        const progress =
          isActive && denom !== undefined && denom > 0
            ? `fetch progress: ${c.blockCount.toLocaleString()}/${denom.toLocaleString()} (${Math.min(100, (c.blockCount / denom) * 100).toFixed(1)}%) blocks complete`
            : `${c.blockCount.toLocaleString()} blocks fetched (Δ +${delta.toLocaleString()} in ${PROGRESS_INTERVAL_MS / 1000}s)`;
        console.log(`[CACHE:${label}] chain ${c.chainId}${tag} — ${progress}`);
      }

      // ---- reclaim / warn ----
      if (autoPrune) {
        // Deterministic identity is REQUIRED before deleting anything: if the genesis
        // hash could not be read, skip pruning rather than risk the wrong chain.
        if (current) {
          // Only reclaim chains explicitly tagged with THIS env. We deliberately do NOT
          // touch untagged chains: a `withData` undeployed run (integration/smoke) loads
          // pre-seeded node data, so its block-1 hash — and thus chain_id — is STABLE across
          // runs, but those suites never run this reporter, so that chain sits UNTAGGED in
          // the shared cache. Pruning untagged chains here would evict that reusable
          // pre-seeded cache on every from-genesis e2e run. From-genesis chains are tagged
          // deterministically (above), so tag-scoped pruning still clears superseded runs.
          const reclaim = chains.filter(
            (c) => c.chainIdHex !== current.chainIdHex && c.envName === label,
          );
          for (const c of reclaim) {
            if (pruned.has(c.chainIdHex)) continue;
            pruned.add(c.chainIdHex);
            const ok = await pruneChain(c.chainIdHex)
              .then(() => true)
              .catch((err) => {
                console.warn(`[CACHE:${label}] failed to prune ${c.chainId}: ${err}`);
                return false;
              });
            if (ok) {
              console.log(
                `[CACHE:${label}] reclaimed superseded chain ${c.chainId}` +
                  ` (${c.blockCount.toLocaleString()} blocks) — kept current chain ${current.chainId}.`,
              );
            }
          }
        }
      } else {
        // Persistent network: a second chain tagged with this env is mis-attribution,
        // not a reset. Never touch the shared data dir (that wipes every env's cache);
        // surface the exact chains with a scoped prune command instead.
        const mislabeled = chains.filter(
          (c) =>
            c.envName === label &&
            c.chainIdHex !== currentChainIdHex &&
            c.chainId !== activeChainId,
        );
        if (mislabeled.length > 0) {
          const ids = mislabeled.map((c) => `${c.chainId} (${c.chainIdHex})`).join(', ');
          console.warn(
            `[CACHE:${label}] ⚠  ${mislabeled.length} unexpected chain(s) tagged \`${label}\`: ${ids}. ` +
              `These look mis-attributed, not ${label} data. ` +
              `Prune only those chains (do NOT delete .tmp/toolkit-postgres-data — that wipes every env's cache):\n` +
              mislabeled
                .map(
                  (c) =>
                    `    docker exec ${CONTAINER_NAME} psql -U ${POSTGRES_USER} -d ${POSTGRES_DB} ` +
                    `-c "DELETE FROM raw_block_data_v2 WHERE chain_id = decode('${c.chainIdHex}','hex'); ` +
                    `DELETE FROM highest_verified WHERE chain_id = decode('${c.chainIdHex}','hex'); ` +
                    `DELETE FROM chain_names WHERE chain_id = decode('${c.chainIdHex}','hex');"`,
                )
                .join('\n'),
          );
        }
      }
    } catch {
      // Non-fatal — reporter runs best-effort alongside warmup
    }
  };

  // Fetch tip immediately so the first tick has a denominator
  void fetchChainTip();
  const handle = setInterval(() => void tick(), PROGRESS_INTERVAL_MS);
  // Fire an initial tick after a short delay so the first row has time to appear
  setTimeout(() => void tick(), 3_000);

  return { stop: () => clearInterval(handle) };
}

async function queryChainProgress(): Promise<ChainProgress[]> {
  const sql = `
    SELECT
      encode(r.chain_id, 'hex')  AS chain_id,
      COUNT(*)::bigint            AS block_count,
      COALESCE(h.height, 0)       AS highest_block,
      COALESCE(n.env_name, '')    AS env_name
    FROM raw_block_data_v2 r
    LEFT JOIN highest_verified h USING (chain_id)
    LEFT JOIN chain_names n USING (chain_id)
    GROUP BY r.chain_id, h.height, n.env_name
    ORDER BY block_count DESC;
  `.trim();

  const { stdout } = await execFileAsync('docker', [
    'exec',
    CONTAINER_NAME,
    'psql',
    '-U',
    POSTGRES_USER,
    '-d',
    POSTGRES_DB,
    '-t',
    '-A',
    '-F',
    '|',
    '-c',
    sql,
  ]);

  return stdout
    .trim()
    .split('\n')
    .filter(Boolean)
    .map((line) => {
      const [chainIdHex, blockCountStr, highestBlockStr, envName] = line.split('|');
      return {
        chainId: `0x${chainIdHex.slice(0, 8)}…`,
        chainIdHex,
        blockCount: parseInt(blockCountStr, 10),
        highestBlock: parseInt(highestBlockStr, 10),
        envName: envName || undefined,
      };
    });
}

/**
 * Remove a single chain's blocks from the shared fetch cache.
 *
 * Deletes only the rows keyed by this `chain_id` across `raw_block_data_v2`,
 * `highest_verified`, and `chain_names`. The shared Postgres container and its
 * `.tmp/toolkit-postgres-data` volume (which holds every env's cache, including
 * the multi-million-block qanet/preview chains) are left fully intact — this is
 * the surgical alternative to wiping the whole data dir.
 *
 * Exported so it can be driven from a one-off cleanup script or test if needed.
 */
export async function pruneChain(chainIdHex: string): Promise<void> {
  await runPsqlScript(
    `DELETE FROM raw_block_data_v2 WHERE chain_id = decode(:'chain_id_hex', 'hex');
     DELETE FROM highest_verified WHERE chain_id = decode(:'chain_id_hex', 'hex');
     DELETE FROM chain_names      WHERE chain_id = decode(:'chain_id_hex', 'hex');`,
    { chain_id_hex: chainIdHex },
  );
}

async function registerChainName(chainIdHex: string, envName: string): Promise<void> {
  await runPsqlScript(
    `INSERT INTO chain_names (chain_id, env_name)
     VALUES (decode(:'chain_id_hex', 'hex'), :'env')
     ON CONFLICT (chain_id) DO NOTHING;`,
    { chain_id_hex: chainIdHex, env: envName },
  );
}

/**
 * Run a psql script inside the cache container, with values passed as psql
 * variables (interpolated via `:'name'`) so they can never be parsed as SQL.
 *
 * The script is fed on stdin (`-f -`) rather than via `-c`: psql performs
 * `:'var'` interpolation only for scripts read from a file/stdin, NOT for `-c`
 * command strings. Using `-c` here silently produced a `syntax error at or
 * near ":"`, which — because callers swallowed the error — meant chain
 * registration and pruning never actually ran. `ON_ERROR_STOP=1` makes any
 * failure surface as a non-zero exit instead of a partial, ignored result.
 */
async function runPsqlScript(sql: string, vars: Record<string, string> = {}): Promise<void> {
  const varArgs = Object.entries(vars).flatMap(([k, v]) => ['-v', `${k}=${v}`]);
  await new Promise<void>((resolve, reject) => {
    const child = spawn(
      'docker',
      [
        'exec',
        '-i',
        CONTAINER_NAME,
        'psql',
        '-U',
        POSTGRES_USER,
        '-d',
        POSTGRES_DB,
        '-v',
        'ON_ERROR_STOP=1',
        ...varArgs,
        '-f',
        '-',
      ],
      { stdio: ['pipe', 'ignore', 'pipe'] },
    );
    let stderr = '';
    child.stderr.on('data', (d) => (stderr += String(d)));
    child.on('error', reject);
    child.on('close', (code) =>
      code === 0
        ? resolve()
        : reject(new Error(`psql exited with code ${code}${stderr ? `: ${stderr.trim()}` : ''}`)),
    );
    child.stdin.end(sql);
  });
}
