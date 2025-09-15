// This file is part of midnightntwrk/midnight-indexer
// Copyright (C) 2025 Midnight Foundation
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
import { existsSync, readFileSync, createWriteStream, WriteStream } from 'fs';
import { join, basename } from 'path';

// This is an hack to have a log file per test file that is created in a
// "session" path. Every time we execute a new test session a new session
// path will be created by the framework globalConfig with the timestamp gathered
// at creation time.
// The session path will be written into a file for the logger to import it,
// avoiding race conditions
const SESSION_PATH_FILE = 'logs/sessionPath';
if (!existsSync(SESSION_PATH_FILE)) {
  throw new Error('Session directory not initialized. Define a globalSetup script to create it.');
}
const SESSION_DIR = readFileSync(SESSION_PATH_FILE, 'utf8').trim();

const SESSION_TS = new Date()
  .toISOString()
  .replace(/T/, '_')
  .replace(/:/g, '-')
  .replace(/\..+/, '');

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
  messageFormat: (log: any, messageKey: string, levelLabel: string) => {
    const lvl = levelLabel.toUpperCase().padEnd(5);
    return `[${log.time}] ${lvl}: ${log[messageKey]}`;
  },
};

type LogLevel = 'TRACE' | 'DEBUG' | 'INFO' | 'WARN' | 'ERROR' | 'FATAL';
const LEVEL = (process.env.LOG_LEVEL ?? 'WARN').toUpperCase() as LogLevel;
const consoleLogger = pino(
  { level: LEVEL, timestamp: () => `,\"time\":\"${new Date().toISOString()}\"` },
  pino.multistream([{ level: LEVEL, stream: pretty({ ...prettyOpts, colorize: true }) }]),
);

// Cache per-test file streams for manual formatting
const fileStreams = new Map<string, WriteStream>();
function getFileStream(testPath: string): WriteStream {
  const name = basename(testPath)
    .replace(/\.[jt]sx?$/, '')
    .replace(/[^\w.-]/g, '_');
  const filePath = join(SESSION_DIR, `${name}.log`);
  if (!fileStreams.has(filePath)) {
    fileStreams.set(filePath, createWriteStream(filePath, { flags: 'a' }));
  }
  return fileStreams.get(filePath)!;
}

// Single logger proxy: console + per-test file (although for some reasons
// the cli logging requires some revising)
const log: Logger = new Proxy(consoleLogger, {
  get(target, prop: string) {
    const orig = (target as any)[prop];
    if (typeof orig !== 'function') {
      return orig;
    }
    return (...args: any[]) => {
      orig.apply(target, args);

      let testPath: string | undefined;
      try {
        testPath = (expect as any).getState().testPath;
      } catch {}

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
