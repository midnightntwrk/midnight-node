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
import { isNodeVersionAtLeast, parseVersionCore } from "./nodeVersion";

test("parseVersionCore reads the core and ignores the commit suffix", () => {
  assert.deepEqual(parseVersionCore("3.0.0-1a2b3c4d"), [3, 0, 0]);
  assert.deepEqual(parseVersionCore("v2.1.0"), [2, 1, 0]);
  assert.equal(parseVersionCore("unknown"), undefined);
});

test("isNodeVersionAtLeast compares major, minor and patch in order", () => {
  assert.equal(isNodeVersionAtLeast("3.0.0-abc", "3.0.0"), true);
  assert.equal(isNodeVersionAtLeast("3.1.0-abc", "3.0.5"), true);
  assert.equal(isNodeVersionAtLeast("2.10.0-abc", "3.0.0"), false);
  assert.equal(isNodeVersionAtLeast("3.0.0-abc", "3.0.1"), false);
});

test("isNodeVersionAtLeast is undefined when a version has no core", () => {
  assert.equal(isNodeVersionAtLeast("unknown", "3.0.0"), undefined);
  assert.equal(isNodeVersionAtLeast("3.0.0", "latest"), undefined);
});
