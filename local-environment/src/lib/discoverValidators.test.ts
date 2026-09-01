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

import {
  discoverValidatorEndpoints,
  discoverValidators,
} from "./discoverValidators";

const tmpFiles: string[] = [];

after(() => {
  for (const f of tmpFiles) fs.rmSync(f, { force: true });
});

/** Write a compose fixture to a temp file and track it for cleanup. */
function fixture(yaml: string): string {
  const file = path.join(
    fs.mkdtempSync(path.join(os.tmpdir(), "discover-validators-")),
    "docker-compose.yaml",
  );
  fs.writeFileSync(file, yaml);
  tmpFiles.push(file);
  return file;
}

describe("discoverValidators", () => {
  it("reads container and host ports from object-form labels + string ports", () => {
    const file = fixture(`
services:
  node1:
    labels:
      io.midnight.role: validator
      io.midnight.rpc-port: 9944
    ports:
      - "9950:9944"
`);
    assert.deepEqual(discoverValidators(file), [
      { name: "node1", rpcPort: 9944, hostRpcPort: 9950 },
    ]);
  });

  it("supports array-form labels and long-form (published/target) ports", () => {
    const file = fixture(`
services:
  node2:
    labels:
      - "io.midnight.role=validator"
      - "io.midnight.rpc-port=9944"
    ports:
      - target: 9944
        published: 9951
`);
    assert.deepEqual(discoverValidators(file), [
      { name: "node2", rpcPort: 9944, hostRpcPort: 9951 },
    ]);
  });

  it("returns one entry per validator and ignores non-validator services", () => {
    const file = fixture(`
services:
  node1:
    labels: { io.midnight.role: validator, io.midnight.rpc-port: 9944 }
    ports: ["9950:9944"]
  node2:
    labels: { io.midnight.role: validator, io.midnight.rpc-port: 9944 }
    ports: ["9951:9944"]
  cardano-db-sync:
    image: some/image
    ports: ["5432:5432"]
`);
    assert.deepEqual(
      discoverValidators(file).map((v) => v.name),
      ["node1", "node2"],
    );
  });

  it("throws when a validator is missing its rpc-port label", () => {
    const file = fixture(`
services:
  node1:
    labels: { io.midnight.role: validator }
    ports: ["9950:9944"]
`);
    assert.throws(
      () => discoverValidators(file),
      /missing 'io\.midnight\.rpc-port'/,
    );
  });

  it("throws when the rpc-port is not published by a ports entry", () => {
    const file = fixture(`
services:
  node1:
    labels: { io.midnight.role: validator, io.midnight.rpc-port: 9944 }
    ports: ["9615:9615"]
`);
    assert.throws(() => discoverValidators(file), /no matching 'ports:' entry/);
  });

  it("throws when no validator services are present", () => {
    const file = fixture(`
services:
  cardano-db-sync:
    image: some/image
`);
    assert.throws(
      () => discoverValidators(file),
      /No validator services found/,
    );
  });
});

describe("discoverValidatorEndpoints", () => {
  it("maps each validator to a host RPC endpoint", () => {
    const file = fixture(`
services:
  node1:
    labels: { io.midnight.role: validator, io.midnight.rpc-port: 9944 }
    ports: ["9950:9944"]
`);
    assert.deepEqual(discoverValidatorEndpoints(file), [
      { name: "node1", url: "http://localhost:9950" },
    ]);
  });
});
