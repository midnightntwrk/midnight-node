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
import { spawn, spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { pathToFileURL } from "node:url";
import YAML from "yaml";

import { restoreSnapshot } from "./snapshotRestore";

// restoreSnapshot downloads with a blocking curl, so the archive has to be
// served from another process.
async function serveDir(t: test.TestContext, dir: string): Promise<string> {
  const server = spawn(
    process.execPath,
    [
      "-e",
      `const fs = require("fs"), path = require("path");
       require("http").createServer((req, res) => {
         fs.createReadStream(path.join(${JSON.stringify(dir)}, req.url)).pipe(res);
       }).listen(0, "127.0.0.1", function () { console.log(this.address().port); });`,
    ],
    { stdio: ["ignore", "pipe", "inherit"] },
  );
  t.after(() => server.kill());
  const port = await new Promise<string>((resolve) =>
    server.stdout.once("data", (chunk) => resolve(String(chunk).trim())),
  );
  return `http://127.0.0.1:${port}`;
}

test("prepares the extracted snapshot once, then replicates it into every data dir", async (t) => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "snapshot-restore-"));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));

  const archiveSrc = path.join(root, "src");
  fs.mkdirSync(path.join(archiveSrc, "node", "chains", "test"), {
    recursive: true,
  });
  fs.writeFileSync(
    path.join(archiveSrc, "node", "chains", "test", "db"),
    "original",
  );
  const served = path.join(root, "served");
  fs.mkdirSync(served);
  assert.equal(
    spawnSync("tar", [
      "-czf",
      path.join(served, "snap.tar.gz"),
      "-C",
      archiveSrc,
      "node",
    ]).status,
    0,
  );

  const composeDir = path.join(root, "compose");
  fs.mkdirSync(composeDir);
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

  const prepared: string[] = [];
  const restored = await restoreSnapshot({
    namespace: "test",
    composeFile,
    snapshotUri: `${await serveDir(t, served)}/snap.tar.gz`,
    env: {},
    stripComponents: 1,
    prepare: (unpackedRoot) => {
      prepared.push(unpackedRoot);
      fs.writeFileSync(
        path.join(unpackedRoot, "chains", "test", "db"),
        "converted",
      );
    },
  });

  assert.equal(prepared.length, 1);
  const data = path.join(composeDir, "data");
  assert.deepEqual(restored, [
    path.join(data, "boot-node"),
    path.join(data, "node-1"),
    path.join(data, "node-2"),
  ]);
  for (const dir of restored) {
    assert.equal(
      fs.readFileSync(path.join(dir, "chains", "test", "db"), "utf-8"),
      "converted",
    );
  }
});

function setup(t: test.TestContext) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "snapshot-restore-"));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));

  const src = path.join(root, "src", "node", "chains", "test");
  fs.mkdirSync(src, { recursive: true });
  fs.writeFileSync(path.join(src, "db"), "state");
  const archive = path.join(root, "archives", "snap.tar.gz");
  fs.mkdirSync(path.dirname(archive));
  const tar = spawnSync("tar", [
    "-czf",
    archive,
    "-C",
    path.join(root, "src"),
    "node",
  ]);
  assert.equal(tar.status, 0);

  const composeFile = path.join(root, "compose", "test.network.yaml");
  fs.mkdirSync(path.dirname(composeFile));
  fs.writeFileSync(
    composeFile,
    YAML.stringify({
      services: {
        node1: { volumes: ["./data/node-1:/data"] },
        node2: { volumes: ["./data/node-2:/data"] },
      },
    }),
  );
  return { archive, composeFile };
}

for (const [name, toUri] of [
  ["path", (p: string) => p],
  ["file:// URL", (p: string) => pathToFileURL(p).href],
] as const) {
  test(`restores a local archive given as a ${name}, leaving it in place`, async (t) => {
    const { archive, composeFile } = setup(t);

    const restored = await restoreSnapshot({
      namespace: "test",
      composeFile,
      snapshotUri: toUri(archive),
      env: {},
      stripComponents: 1,
    });

    assert.equal(restored.length, 2);
    for (const dir of restored) {
      assert.equal(
        fs.readFileSync(path.join(dir, "chains", "test", "db"), "utf-8"),
        "state",
      );
    }
    assert.deepEqual(fs.readdirSync(path.dirname(archive)), ["snap.tar.gz"]);
  });
}

test("rejects a local archive that does not exist", async (t) => {
  const { archive, composeFile } = setup(t);

  await assert.rejects(
    restoreSnapshot({
      namespace: "test",
      composeFile,
      snapshotUri: `${archive}.missing`,
      env: {},
    }),
    /Snapshot archive not found/,
  );
});

test("stages next to the data dirs and cleans up", async (t) => {
  const { archive, composeFile } = setup(t);
  const dataRoot = path.join(path.dirname(composeFile), "data");
  const log = t.mock.method(console, "log", () => undefined);

  await restoreSnapshot({
    namespace: "test",
    composeFile,
    snapshotUri: archive,
    env: {},
    stripComponents: 1,
  });

  const extracting = log.mock.calls
    .map((call) => String(call.arguments[0]))
    .find((line) => line.startsWith("Extracting "));
  assert.ok(
    extracting?.includes(path.join(dataRoot, ".snapshot-staging-")),
    extracting,
  );
  assert.deepEqual(fs.readdirSync(dataRoot).sort(), ["node-1", "node-2"]);
});
