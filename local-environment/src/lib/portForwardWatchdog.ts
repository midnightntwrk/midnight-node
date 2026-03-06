// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
// http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

import { spawn } from "child_process";
import fs from "fs";
import os from "os";
import path from "path";

const DEFAULT_BASE_DELAY_MS = 1000;
const DEFAULT_MAX_DELAY_MS = 30000;
const PID_PREFIX = "midnight-port-forward";
const PID_SUFFIX = ".pid";

const WATCHDOG_SCRIPT = path.resolve(__dirname, "portForwardWatchdogRunner.js");

export interface PortForwardSpec {
  namespace: string;
  podName: string;
  localPort: number;
  remotePort: number;
  label?: string;
}

export interface PortForwardRetryOptions {
  baseDelayMs?: number;
  maxDelayMs?: number;
}

export function startPortForwardWatchdog(
  spec: PortForwardSpec,
  options: PortForwardRetryOptions = {},
) {
  const pidFile = buildPidFilePath(spec);
  const env = {
    ...process.env,
    MIDNIGHT_PORT_FORWARD_SPEC: JSON.stringify(spec),
    MIDNIGHT_PORT_FORWARD_BASE_DELAY_MS: String(
      options.baseDelayMs ?? DEFAULT_BASE_DELAY_MS,
    ),
    MIDNIGHT_PORT_FORWARD_MAX_DELAY_MS: String(
      options.maxDelayMs ?? DEFAULT_MAX_DELAY_MS,
    ),
    MIDNIGHT_PORT_FORWARD_PID_FILE: pidFile,
  };

  const watchdog = spawn(process.execPath, [WATCHDOG_SCRIPT], {
    detached: true,
    stdio: "inherit",
    env,
  });

  watchdog.unref();
}

export function stopPortForwardWatchdogs(namespace?: string) {
  const pidFiles = listPidFiles(namespace);
  if (pidFiles.length === 0) {
    return;
  }

  for (const pidFile of pidFiles) {
    const pid = readPidFile(pidFile);
    if (!pid) {
      removePidFile(pidFile);
      continue;
    }

    try {
      process.kill(pid, "SIGTERM");
      console.log(`Stopping port-forward watchdog pid ${pid}`);
    } catch (error) {
      console.warn(
        `Failed to stop watchdog pid ${pid}: ${(error as Error).message}`,
      );
    }
    removePidFile(pidFile);
  }
}

function encodeSegment(value: string): string {
  return Buffer.from(value, "utf-8")
    .toString("base64")
    .replace(/\+/g, "-")
    .replace(/\//g, "_")
    .replace(/=+$/g, "");
}

function buildPidFilePath(spec: PortForwardSpec): string {
  const namespace = encodeSegment(spec.namespace);
  const podName = encodeSegment(spec.podName);
  const fileName = `${PID_PREFIX}.${namespace}.${podName}.${spec.localPort}${PID_SUFFIX}`;
  return path.join(os.tmpdir(), fileName);
}

function listPidFiles(namespace?: string): string[] {
  let entries: string[];
  try {
    entries = fs.readdirSync(os.tmpdir());
  } catch (error) {
    console.warn(`Failed to list pid files: ${(error as Error).message}`);
    return [];
  }

  const namespaceEnc = namespace ? encodeSegment(namespace) : undefined;
  const matches: string[] = [];

  for (const entry of entries) {
    const parsed = parsePidFileName(entry);
    if (!parsed) {
      continue;
    }
    if (namespaceEnc && parsed.namespaceEnc !== namespaceEnc) {
      continue;
    }
    matches.push(path.join(os.tmpdir(), entry));
  }

  return matches;
}

function parsePidFileName(fileName: string): {
  namespaceEnc: string;
  podEnc: string;
  port: number;
} | null {
  if (!fileName.startsWith(`${PID_PREFIX}.`)) {
    return null;
  }
  if (!fileName.endsWith(PID_SUFFIX)) {
    return null;
  }

  const base = fileName.slice(0, -PID_SUFFIX.length);
  const parts = base.split(".");
  if (parts.length !== 4) {
    return null;
  }

  const port = Number(parts[3]);
  if (!Number.isFinite(port)) {
    return null;
  }

  return {
    namespaceEnc: parts[1],
    podEnc: parts[2],
    port,
  };
}

function readPidFile(pidFile: string): number | undefined {
  try {
    const raw = fs.readFileSync(pidFile, "utf-8").trim();
    const pid = Number(raw);
    return Number.isFinite(pid) ? pid : undefined;
  } catch (error) {
    console.warn(
      `Failed to read pid file '${pidFile}': ${(error as Error).message}`,
    );
    return undefined;
  }
}

function removePidFile(pidFile: string) {
  try {
    fs.rmSync(pidFile, { force: true });
  } catch (error) {
    console.warn(
      `Failed to remove pid file '${pidFile}': ${(error as Error).message}`,
    );
  }
}
