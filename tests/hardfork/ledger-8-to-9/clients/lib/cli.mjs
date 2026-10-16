// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

import util from 'node:util';

// Progress goes to stderr, the result as JSON to stdout. Exit 0 pass, 1 failed, 2 usage.
export const log = (...args) => console.error('[client]', ...args);

export const emit = (obj) => {
  process.stdout.write(JSON.stringify(obj, (_k, v) => (typeof v === 'bigint' ? v.toString() : v), 2) + '\n');
};

export class UsageError extends Error {}

const args = process.argv.slice(2);

export const flag = (name, fallback) => {
  const i = args.indexOf(`--${name}`);
  return i === -1 ? fallback : args[i + 1];
};

const integer = (what, raw, min) => {
  const n = Number(raw);
  if (raw === '' || !Number.isSafeInteger(n) || n < min) throw new UsageError(`${what} must be an integer >= ${min}, got '${raw}'`);
  return n;
};
export const intFlag = (name, fallback, min = 0) => {
  const raw = flag(name);
  return raw === undefined ? fallback : integer(`--${name}`, raw, min);
};
export const intEnv = (name, fallback, min = 0) => {
  const raw = process.env[name];
  return raw === undefined ? fallback : integer(name, raw, min);
};

export const withTimeout = (promise, secs, what) => {
  let timer;
  const timeout = new Promise((_resolve, reject) => {
    timer = setTimeout(() => reject(new Error(`${what}: no result after ${secs} s`)), secs * 1000);
  });
  return Promise.race([promise, timeout]).finally(() => clearTimeout(timer));
};
export const txTimeoutSecs = () => intEnv('MN_TX_TIMEOUT_SECS', 600, 1);

// The node's verdict on a refused transaction, from anywhere in the cause chain:
// "1010: Invalid Transaction: Custom error: 170".
const messages = (err, seen = new Set()) => {
  if (!err || typeof err !== 'object' || seen.has(err) || ArrayBuffer.isView(err)) return [];
  seen.add(err);
  const own = typeof err.message === 'string' ? [err.message] : [];
  const nested = [...Object.values(err), ...Object.getOwnPropertySymbols(err).map((k) => err[k])];
  return [...own, ...nested.flatMap((v) => messages(v, seen))];
};
const nodeRefusal = (err) => {
  const m = messages(err).map((s) => s.match(/\b(10\d\d): ([^:]+)(?:: Custom error: (\d+))?/)).find(Boolean);
  return m ? { rpcCode: Number(m[1]), reason: m[2], ...(m[3] && { customError: Number(m[3]) }) } : {};
};

// The process exits only once stdout has drained: an exit right after a write to a pipe
// truncates the JSON at 64 KB.
let finished = false;
const finish = (code) => {
  finished = true;
  process.exitCode = code;
  process.stdout.write('', () => process.exit(code));
};
const fail = (err) => {
  if (finished) { log('after the result:', util.inspect(err, { depth: 2 })); return; }
  const usage = err instanceof UsageError;
  const name = err?.name ?? 'Error';
  const message = String(err?.message ?? err);
  log(`${usage ? 'usage' : 'FAIL'}: ${name}: ${message}`);
  if (err?.cause) log(`  cause: ${err.cause?.name ?? ''} ${err.cause?.message ?? err.cause}`);
  if (!usage) emit({ error: { name, message, ...nodeRefusal(err) } });
  finish(usage ? 2 : 1);
};

export const runCommands = (commands) => {
  process.on('unhandledRejection', fail);
  process.on('uncaughtException', fail);
  const fn = commands[args[0]];
  if (!fn) {
    log(`unknown command '${args[0] ?? ''}'. One of: ${Object.keys(commands).join(', ')}`);
    finish(2);
    return;
  }
  Promise.resolve()
    .then(() => {
      const secs = intEnv('MN_TIMEOUT_SECS', 1800, 1);
      return withTimeout(Promise.resolve().then(fn), secs, `${args[0]} (MN_TIMEOUT_SECS; a transaction it sent may still land)`);
    })
    .then((code) => finish(code ?? 0), fail);
};
