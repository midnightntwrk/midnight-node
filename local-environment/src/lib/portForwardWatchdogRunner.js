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

/* eslint-disable @typescript-eslint/no-require-imports */
const fs = require("fs");
const { spawn } = require("child_process");

const specJson = process.env.MIDNIGHT_PORT_FORWARD_SPEC;
const logKubectl = process.env.MIDNIGHT_KUBECTL_LOG === "1";
const pidFile = process.env.MIDNIGHT_PORT_FORWARD_PID_FILE;
if (!specJson) {
  console.error("Missing MIDNIGHT_PORT_FORWARD_SPEC");
  process.exit(1);
}

let spec;
try {
  spec = JSON.parse(specJson);
} catch (error) {
  const message = error && error.message ? error.message : String(error);
  console.error("Invalid MIDNIGHT_PORT_FORWARD_SPEC:", message);
  process.exit(1);
}

const namespace = spec.namespace;
const podName = spec.podName;
const localPort = Number(spec.localPort);
const remotePort = Number(spec.remotePort);

if (!namespace || !podName || !Number.isFinite(localPort) || !Number.isFinite(remotePort)) {
  console.error("Invalid port-forward spec:", spec);
  process.exit(1);
}

const label = spec.label || namespace + "/" + podName;
const baseDelayMs = parseInt(process.env.MIDNIGHT_PORT_FORWARD_BASE_DELAY_MS || "1000", 10);
const maxDelayMs = parseInt(process.env.MIDNIGHT_PORT_FORWARD_MAX_DELAY_MS || "30000", 10);

let retry = 0;
let child = null;
let stopping = false;

function writePidFile() {
  if (!pidFile) {
    return;
  }
  try {
    fs.writeFileSync(pidFile, String(process.pid));
  } catch (error) {
    console.warn("Failed to write pid file:", error.message);
  }
}

function cleanupPidFile() {
  if (!pidFile) {
    return;
  }
  try {
    fs.rmSync(pidFile, { force: true });
  } catch (error) {
    console.warn("Failed to remove pid file:", error.message);
  }
}

function log(message) {
  console.log("[port-forward " + label + "] " + message);
}

function nextDelay() {
  const capped = Math.min(Math.max(retry - 1, 0), 6);
  return Math.min(maxDelayMs, baseDelayMs * Math.pow(2, capped));
}

function start() {
  if (stopping) {
    return;
  }
  const args = ["-n", namespace, "port-forward", "pod/" + podName, localPort + ":" + remotePort];
  if (logKubectl) {
    log("exec: kubectl " + args.join(" "));
  }
  log("starting on localhost:" + localPort + " -> " + remotePort);
  child = spawn("kubectl", args, { stdio: "inherit" });
  child.on("exit", function (code, signal) {
    if (stopping) {
      return;
    }
    retry += 1;
    const delay = nextDelay();
    log("kubectl exited (code " + code + ", signal " + signal + "); retrying in " + delay + "ms");
    setTimeout(start, delay);
  });
  child.on("error", function (error) {
    if (stopping) {
      return;
    }
    retry += 1;
    const delay = nextDelay();
    log("spawn failed: " + error.message + "; retrying in " + delay + "ms");
    setTimeout(start, delay);
  });
}

function shutdown() {
  stopping = true;
  if (child && !child.killed) {
    child.kill("SIGTERM");
  }
  cleanupPidFile();
  setTimeout(function () {
    process.exit(0);
  }, 1000);
}

process.on("exit", cleanupPidFile);
process.on("SIGTERM", shutdown);
process.on("SIGINT", shutdown);

writePidFile();
start();
