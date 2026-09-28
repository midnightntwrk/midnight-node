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

import {
  DISABLED_MOCK_VALIDATOR_PROFILE,
  generateMockComposeOverride,
  readMockValidatorSelection,
} from "./mockComposeOverride";
import { MockModeConfig, resolveMockValidatorSelection } from "./networkConfig";

const config: MockModeConfig = {
  chainId: "midnight_test",
  numValidators: 2,
  validatorServices: ["node1", "node2", "node3"],
  extraServices: ["boot-node"],
};

test("uses the configured validator count by default", () => {
  assert.deepEqual(resolveMockValidatorSelection(config), {
    numValidators: 2,
    validatorServices: ["node1", "node2"],
    disabledValidatorServices: ["node3"],
  });
});

test("a requested count overrides the configured default", () => {
  assert.deepEqual(resolveMockValidatorSelection(config, 1), {
    numValidators: 1,
    validatorServices: ["node1"],
    disabledValidatorServices: ["node2", "node3"],
  });
});

test("rejects counts outside the configured Compose topology", () => {
  assert.throws(
    () => resolveMockValidatorSelection(config, 0),
    /must be a positive integer/,
  );
  assert.throws(
    () => resolveMockValidatorSelection(config, 1.5),
    /must be a positive integer/,
  );
  assert.throws(
    () => resolveMockValidatorSelection(config, 4),
    /only defines 3/,
  );
});

test("persists the active selection and disables unselected services", (t) => {
  const composeDir = fs.mkdtempSync(
    path.join(os.tmpdir(), "mock-validator-selection-"),
  );
  t.after(() => fs.rmSync(composeDir, { recursive: true, force: true }));

  const mockedConfigDir = path.join(composeDir, "mocked-config");
  fs.mkdirSync(mockedConfigDir, { recursive: true });
  fs.writeFileSync(path.join(mockedConfigDir, "mock-registrations.json"), "{}");

  const overridePath = generateMockComposeOverride({
    composeDir,
    network: "testnet",
    validatorServices: ["node1", "node2"],
    disabledValidatorServices: ["node3"],
    extraServices: ["boot-node"],
  });

  assert.deepEqual(readMockValidatorSelection(overridePath), {
    numValidators: 2,
    validatorServices: ["node1", "node2"],
    disabledValidatorServices: ["node3"],
  });

  const override = YAML.parse(fs.readFileSync(overridePath, "utf-8"));
  assert.deepEqual(override.services.node3.profiles, [
    DISABLED_MOCK_VALIDATOR_PROFILE,
  ]);
  assert.equal(
    override.services.node1.volumes[0],
    "./mocked-config/seeds/validator-0:/seeds",
  );
  assert.equal(
    override.services.node2.volumes[0],
    "./mocked-config/seeds/validator-1:/seeds",
  );
});
