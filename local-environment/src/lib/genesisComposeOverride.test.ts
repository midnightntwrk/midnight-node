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
import { test } from "node:test";
import fs from "fs";
import os from "os";
import path from "path";
import YAML from "yaml";
import {
  generateGenesisComposeOverride,
  genesisOverridePath,
  GENESIS_CONFIG_DIRNAME,
} from "./genesisComposeOverride";

/** Writes a compose file with the given services into a fresh temp dir. */
function writeCompose(services: Record<string, unknown>): string {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "genesis-override-test-"));
  const composeFile = path.join(dir, "testnet.network.yaml");
  fs.writeFileSync(composeFile, YAML.stringify({ services }));
  return composeFile;
}

function readSeed(composeFile: string, service: string, file: string): string {
  return fs.readFileSync(
    path.join(
      path.dirname(composeFile),
      GENESIS_CONFIG_DIRNAME,
      "seeds",
      service,
      file,
    ),
    "utf8",
  );
}

test("wires seed files for services whose seed var is set", () => {
  const composeFile = writeCompose({
    node1: { environment: { SEED_PHRASE: "$NODE_01_SEED" } },
    node2: { environment: { SEED_PHRASE: "${NODE_02_SEED}" } },
    proxy: { environment: { OTHER: "value" } },
  });

  const result = generateGenesisComposeOverride({
    composeFile,
    network: "testnet",
    env: { NODE_01_SEED: "one two three" },
  });

  assert.deepEqual(result.seededServices, ["node1"]);
  assert.deepEqual(result.missingSeedVars, { node2: "NODE_02_SEED" });
  assert.equal(
    result.overridePath,
    genesisOverridePath(path.dirname(composeFile), "testnet"),
  );

  const override = YAML.parse(fs.readFileSync(result.overridePath, "utf8"));
  assert.deepEqual(override.services.node1, {
    environment: {
      AURA_SEED_FILE: "/seeds/aura.seed",
      GRANDPA_SEED_FILE: "/seeds/grandpa.seed",
      CROSS_CHAIN_SEED_FILE: "/seeds/cross_chain.seed",
      SEED_PHRASE: "",
    },
    volumes: [`./${GENESIS_CONFIG_DIRNAME}/seeds/node1:/seeds:ro`],
  });
  assert.equal(override.services.node2, undefined);
  assert.equal(override.services.proxy, undefined);

  for (const file of ["aura.seed", "grandpa.seed", "cross_chain.seed"]) {
    assert.equal(readSeed(composeFile, "node1", file), "one two three");
  }
});

test("per-key-type vars override the base var and fall back to it", () => {
  const composeFile = writeCompose({
    node1: { environment: { SEED_PHRASE: "$NODE_01_SEED" } },
  });

  const result = generateGenesisComposeOverride({
    composeFile,
    network: "testnet",
    env: { NODE_01_SEED: "base phrase", NODE_01_AURA_SEED: "aura phrase" },
  });

  assert.deepEqual(result.seededServices, ["node1"]);
  assert.equal(readSeed(composeFile, "node1", "aura.seed"), "aura phrase");
  assert.equal(readSeed(composeFile, "node1", "grandpa.seed"), "base phrase");
  assert.equal(
    readSeed(composeFile, "node1", "cross_chain.seed"),
    "base phrase",
  );
});

test("incomplete per-type seeds without a base-var fallback are not seeded", () => {
  const composeFile = writeCompose({
    node1: { environment: { SEED_PHRASE: "$NODE_01_SEED" } },
  });

  const result = generateGenesisComposeOverride({
    composeFile,
    network: "testnet",
    env: { NODE_01_AURA_SEED: "aura only" },
  });

  assert.deepEqual(result.seededServices, []);
  assert.deepEqual(result.missingSeedVars, { node1: "NODE_01_SEED" });
});

test("a SEED_PHRASE that is not a plain env var reference is skipped", () => {
  const composeFile = writeCompose({
    node1: { environment: { SEED_PHRASE: "inline literal phrase" } },
  });

  const result = generateGenesisComposeOverride({
    composeFile,
    network: "testnet",
    env: {},
  });

  assert.deepEqual(result.seededServices, []);
  assert.deepEqual(result.missingSeedVars, {});
});
