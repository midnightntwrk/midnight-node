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

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import YAML from "yaml";

import { syncConvertedDataToExtraServices } from "./snapshotRestore";

const chainId = "midnight_test";

function writeFile(file: string, content: string) {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, content);
}

function setupFork(t: test.TestContext) {
  const composeDir = fs.mkdtempSync(path.join(os.tmpdir(), "snapshot-sync-"));
  t.after(() => fs.rmSync(composeDir, { recursive: true, force: true }));

  const composeFile = path.join(composeDir, "test.network.yaml");
  fs.writeFileSync(
    composeFile,
    YAML.stringify({
      services: {
        "boot-node": { volumes: ["./data/boot-node:/data"] },
        node1: { volumes: ["./data/node-1:/data", "./res:/res"] },
        node2: { volumes: ["./data/node-2:/data"] },
      },
    }),
  );

  const data = path.join(composeDir, "data");
  for (const node of ["node-1", "node-2"]) {
    writeFile(
      path.join(data, node, "chains", chainId, "paritydb", "full", "db"),
      "converted",
    );
    writeFile(
      path.join(data, node, "chains", chainId, "keystore", "key"),
      `${node}-secret`,
    );
    writeFile(path.join(data, node, "ledger_storage", "db"), "converted");
  }
  writeFile(
    path.join(data, "boot-node", "chains", chainId, "paritydb", "full", "db"),
    "original",
  );
  writeFile(path.join(data, "boot-node", "stale"), "original");

  return { composeFile, data };
}

test("copies converted validator data into extra services without the keystore", (t) => {
  const { composeFile, data } = setupFork(t);
  const bootNode = path.join(data, "boot-node");

  const synced = syncConvertedDataToExtraServices({
    composeFile,
    chainId,
    sourceService: "node1",
    extraServices: ["boot-node"],
  });

  assert.deepEqual(synced, [bootNode]);
  const read = (...p: string[]) =>
    fs.readFileSync(path.join(bootNode, ...p), "utf-8");
  assert.equal(read("chains", chainId, "paritydb", "full", "db"), "converted");
  assert.equal(read("ledger_storage", "db"), "converted");
  assert.equal(fs.existsSync(path.join(bootNode, "stale")), false);
  assert.equal(
    fs.existsSync(path.join(bootNode, "chains", chainId, "keystore")),
    false,
  );
  // The source validator keeps its keys.
  assert.equal(
    fs.readFileSync(
      path.join(data, "node-1", "chains", chainId, "keystore", "key"),
      "utf-8",
    ),
    "node-1-secret",
  );
});

test("is a no-op without extra services", (t) => {
  const { composeFile } = setupFork(t);
  assert.deepEqual(
    syncConvertedDataToExtraServices({
      composeFile,
      chainId,
      sourceService: "node1",
      extraServices: [],
    }),
    [],
  );
});

test("fails when the source validator has no data mount", (t) => {
  const { composeFile } = setupFork(t);
  assert.throws(
    () =>
      syncConvertedDataToExtraServices({
        composeFile,
        chainId,
        sourceService: "node9",
        extraServices: ["boot-node"],
      }),
    /No data dir found for validator service 'node9'/,
  );
});
