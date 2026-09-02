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

import pino, { Logger } from 'pino';
import pretty from 'pino-pretty';
import {
  existsSync,
  readFileSync,
  createWriteStream,
  WriteStream,
  mkdirSync,
  writeFileSync,
} from 'fs';
import { join, basename } from 'path';

// This is an hack to have a log file per test file that is created in a
// "session" path. Every time we execute a new test session a new session
// path will be created by the framework globalConfig with the timestamp gathered
// at creation time.
// The session path will be written into a file for the logger to import it,
// avoiding race conditions
const SESSION_PATH_FILE = 'logs/sessionPath';

// Resolve the session directory, self-healing if it is missing.
//
// The session dir is normally created by the logging globalSetup
// (utils/logging/setup.ts). But that setup imports this module (via
// environment/model), so the import below runs *before* setup() executes — and
// on a fresh checkout/worktree, where `logs/sessionPath` does not yet exist, a
// hard throw here aborts the whole run before globalSetup can create it. (It
// only ever "worked" because a previous run had left the file behind.)
//
// Instead of throwing, create a default session dir so a first run on a clean
// worktree works with no manual seeding. globalSetup still runs afterwards and
// overwrites `sessionPath` with its canonical timestamped dir.
function resolveSessionDir(): string {
  if (existsSync(SESSION_PATH_FILE)) {
    return readFileSync(SESSION_PATH_FILE, 'utf8').trim();
  }
  const base = 'logs';
  if (!existsSync(base)) {
    mkdirSync(base, { recursive: true });
  }
  const ts = new Date().toISOString().replace(/T/, '_').replace(/:/g, '-').replace(/\..+/, '');
  const sessionDir = join(base, ts);
  if (!existsSync(sessionDir)) {
    mkdirSync(sessionDir, { recursive: true });
  }
  writeFileSync(SESSION_PATH_FILE, sessionDir, 'utf8');
  return sessionDir;
}

// Resolve lazily and memoize. Deferring resolution until the first log write
// lets globalSetup run first and point `sessionPath` at its canonical
// timestamped dir, so a fresh checkout no longer leaves an empty orphaned
// `logs/<ts>/` behind from an import-time self-heal.
let sessionDir: string | undefined;
function getSessionDir(): string {
  return (sessionDir ??= resolveSessionDir());
}

// Can we do this differently, lookout for a library that can help with this
// I mean this works but it's quite horrible
function formatTime(): string {
  const d = new Date();
  const dd = String(d.getDate()).padStart(2, '0');
  const mm = String(d.getMonth() + 1).padStart(2, '0');
  const yy = String(d.getFullYear()).slice(-2);
  const hh = String(d.getHours()).padStart(2, '0');
  const mi = String(d.getMinutes()).padStart(2, '0');
  const ss = String(d.getSeconds()).padStart(2, '0');
  return `${dd}-${mm}-${yy} ${hh}:${mi}:${ss}`;
}

const prettyOpts = {
  translateTime: 'SYS:dd-mm-yy HH:MM:ss',
  ignore: 'pid,hostname',
  messageFormat: (log: unknown, messageKey: string, levelLabel: string) => {
    const lvl = levelLabel.toUpperCase().padEnd(5);
    return `[${(log as { time: string }).time}] ${lvl}: ${(log as Record<string, unknown>)[messageKey]}`;
  },
};

type LogLevel = 'TRACE' | 'DEBUG' | 'INFO' | 'WARN' | 'ERROR' | 'FATAL';
const LEVEL = (process.env.LOG_LEVEL ?? 'WARN').toUpperCase() as LogLevel;
const consoleLogger = pino(
  { level: LEVEL, timestamp: () => `,"time":"${new Date().toISOString()}"` },
  pino.multistream([{ level: LEVEL, stream: pretty({ ...prettyOpts, colorize: true }) }]),
);

// Cache per-test file streams for manual formatting
const fileStreams = new Map<string, WriteStream>();
function getFileStream(testPath: string): WriteStream {
  const name = basename(testPath)
    .replace(/\.[jt]sx?$/, '')
    .replace(/[^\w.-]/g, '_');
  const filePath = join(getSessionDir(), `${name}.log`);
  if (!fileStreams.has(filePath)) {
    fileStreams.set(filePath, createWriteStream(filePath, { flags: 'a' }));
  }
  return fileStreams.get(filePath)!;
}

// Single logger proxy: console + per-test file (although for some reasons
// the cli logging requires some revising)
const log: Logger = new Proxy(consoleLogger, {
  get(target, prop: string) {
    const orig = (target as unknown as Record<string, unknown>)[prop];
    if (typeof orig !== 'function') {
      return orig;
    }
    return (...args: unknown[]) => {
      orig.apply(target, args);

      let testPath: string | undefined;
      try {
        testPath = (expect as unknown as { getState: () => { testPath: string } }).getState()
          .testPath;
      } catch {
        // Intentionally ignore - testPath will remain undefined if not in test context
      }

      if (testPath) {
        const stream = getFileStream(testPath);
        const msg = args.map((a) => (typeof a === 'string' ? a : JSON.stringify(a))).join(' ');
        const lvl = prop.toUpperCase().padEnd(5);
        const ts = formatTime();
        stream.write(`[${ts}] ${lvl}: ${msg}\n`);
      }
    };
  },
});

export default log;
