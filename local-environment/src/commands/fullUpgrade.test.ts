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
import { describe, it } from "node:test";

import { fullUpgrade } from "./fullUpgrade";
import { FullUpgradeOptions } from "../lib/types";

describe("fullUpgrade", () => {
  it("rejects --wasm combined with --wasm-from-image before rolling any image", async () => {
    const opts: FullUpgradeOptions = {
      wasmPath: "a.wasm",
      wasmFromImage: "node:new",
      councilUris: ["//Dave"],
      techCommitteeUris: ["//Alice"],
      motionExecutorUri: "//Alice",
    };
    // If this ever reached phase 1, it would try to shell out to a real
    // docker-compose bring-up and hang/fail for unrelated reasons; the point
    // of this test is that it never gets there.
    await assert.rejects(
      () => fullUpgrade("local-env", opts),
      /--wasm and --wasm-from-image are mutually exclusive/,
    );
  });
});
