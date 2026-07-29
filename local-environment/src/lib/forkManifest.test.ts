// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
// http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { after, describe, it } from "node:test";

import { forkManifestPath, writeForkManifest } from "./forkManifest";

const cleanup: string[] = [];

after(() => {
  for (const f of cleanup) fs.rmSync(f, { force: true });
});

/** Write a compose fixture under a named project dir so the compose-derived
 *  network name (`<dir>_default`) is deterministic. */
function fixture(projectDir: string, yaml: string): string {
  const dir = path.join(
    fs.mkdtempSync(path.join(os.tmpdir(), "fork-manifest-")),
    projectDir,
  );
  fs.mkdirSync(dir, { recursive: true });
  const file = path.join(dir, "docker-compose.yaml");
  fs.writeFileSync(file, yaml);
  return file;
}

/** Parse a manifest env file into a key→value map, ignoring comments/blanks. */
function parseManifest(namespace: string): Record<string, string> {
  const file = forkManifestPath(namespace);
  cleanup.push(file);
  const out: Record<string, string> = {};
  for (const line of fs.readFileSync(file, "utf-8").split("\n")) {
    if (!line || line.startsWith("#")) continue;
    const idx = line.indexOf("=");
    out[line.slice(0, idx)] = line.slice(idx + 1);
  }
  return out;
}

const TWO_VALIDATORS = `
services:
  node1:
    labels: { io.midnight.role: validator, io.midnight.rpc-port: 9944 }
    ports: ["9950:9944"]
  node2:
    labels: { io.midnight.role: validator, io.midnight.rpc-port: 9944 }
    ports: ["9951:9944"]
networks:
  default:
    name: midnight-fork-mainnet
`;

describe("writeForkManifest", () => {
  it("emits the full downstream key contract for a named network", () => {
    // These keys are load-bearing for downstream consumers (see midnight-indexer
    // docker-compose.midnight-fork.yaml). A rename here is a breaking change.
    const namespace = "__test_mainnet__";
    const composeFile = fixture("mainnet", TWO_VALIDATORS);
    writeForkManifest({
      namespace,
      composeFile,
      env: { NODE_IMAGE: "ghcr.io/midnight-ntwrk/midnight-node:1.0.0" },
    });

    assert.deepEqual(parseManifest(namespace), {
      MIDNIGHT_FORK_NAMESPACE: "__test_mainnet__",
      MIDNIGHT_FORK_NETWORK: "midnight-fork-mainnet",
      MIDNIGHT_FORK_NETWORK_ID: "__test_mainnet__",
      MIDNIGHT_FORK_NODE_IMAGE: "ghcr.io/midnight-ntwrk/midnight-node:1.0.0",
      MIDNIGHT_FORK_NODE_TAG: "1.0.0",
      MIDNIGHT_FORK_NODE_WS: "ws://node1:9944",
      MIDNIGHT_FORK_NODE_WS_HOST: "ws://localhost:9950",
      MIDNIGHT_FORK_VALIDATORS: "node1,node2",
      MIDNIGHT_FORK_NODE1_WS: "ws://node1:9944",
      MIDNIGHT_FORK_NODE1_WS_HOST: "ws://localhost:9950",
      MIDNIGHT_FORK_NODE2_WS: "ws://node2:9944",
      MIDNIGHT_FORK_NODE2_WS_HOST: "ws://localhost:9951",
    });
  });

  it("uses the primary (first) validator for the bare NODE_WS keys", () => {
    const namespace = "__test_primary__";
    const composeFile = fixture("mainnet", TWO_VALIDATORS);
    writeForkManifest({
      namespace,
      composeFile,
      env: { NODE_IMAGE: "img:tag" },
    });
    const m = parseManifest(namespace);
    assert.equal(m.MIDNIGHT_FORK_NODE_WS, m.MIDNIGHT_FORK_NODE1_WS);
    assert.equal(m.MIDNIGHT_FORK_NODE_WS_HOST, m.MIDNIGHT_FORK_NODE1_WS_HOST);
  });

  it("falls back to the compose-derived network name when none is declared", () => {
    const namespace = "__test_fallback__";
    const composeFile = fixture(
      "someproj",
      `
services:
  node1:
    labels: { io.midnight.role: validator, io.midnight.rpc-port: 9944 }
    ports: ["9950:9944"]
`,
    );
    writeForkManifest({
      namespace,
      composeFile,
      env: { NODE_IMAGE: "img:tag" },
    });
    assert.equal(
      parseManifest(namespace).MIDNIGHT_FORK_NETWORK,
      "someproj_default",
    );
  });

  it("leaves NODE_TAG empty when the image has no tag", () => {
    const namespace = "__test_notag__";
    const composeFile = fixture("mainnet", TWO_VALIDATORS);
    writeForkManifest({
      namespace,
      composeFile,
      env: { NODE_IMAGE: "midnight-node" },
    });
    const m = parseManifest(namespace);
    assert.equal(m.MIDNIGHT_FORK_NODE_IMAGE, "midnight-node");
    assert.equal(m.MIDNIGHT_FORK_NODE_TAG, "");
  });
});
