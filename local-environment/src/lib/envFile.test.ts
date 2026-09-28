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
import { afterEach, describe, it } from "node:test";

import { applyEnvFileOverrides, cleanEnv } from "./envFile";

const tmpDirs: string[] = [];

afterEach(() => {
  for (const dir of tmpDirs.splice(0)) {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

function envFile(contents: string): string {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "env-file-"));
  tmpDirs.push(dir);
  const file = path.join(dir, ".env");
  fs.writeFileSync(file, contents);
  return file;
}

describe("cleanEnv", () => {
  it("drops keys with an undefined value", () => {
    assert.deepEqual(cleanEnv({ A: "1", B: undefined }), { A: "1" });
  });
});

describe("applyEnvFileOverrides", () => {
  it("layers a single env file onto the base", () => {
    const file = envFile("NEW_NODE_IMAGE=ghcr.io/midnight-ntwrk/midnight-node:new\n");
    assert.deepEqual(applyEnvFileOverrides({ NODE_IMAGE: "old" }, [file]), {
      NODE_IMAGE: "old",
      NEW_NODE_IMAGE: "ghcr.io/midnight-ntwrk/midnight-node:new",
    });
  });

  it("lets a later file override an earlier one, and both override the base", () => {
    const first = envFile("A=first\nB=first\n");
    const second = envFile("B=second\n");
    assert.deepEqual(applyEnvFileOverrides({ A: "base", B: "base" }, [first, second]), {
      A: "first",
      B: "second",
    });
  });

  it("warns and skips a missing file instead of throwing", () => {
    const warnings: string[] = [];
    const originalWarn = console.warn;
    console.warn = (msg: string) => warnings.push(msg);
    try {
      const result = applyEnvFileOverrides({ A: "base" }, ["/no/such/file.env"]);
      assert.deepEqual(result, { A: "base" });
    } finally {
      console.warn = originalWarn;
    }
    assert.ok(warnings.some((w) => w.includes("/no/such/file.env")));
  });

  it("returns the base unchanged when no env files are given", () => {
    assert.deepEqual(applyEnvFileOverrides({ A: "base" }, undefined), { A: "base" });
  });
});
