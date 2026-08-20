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
import { collectUnsetComposeVars, run } from "./run";

test("collectUnsetComposeVars reports unset or blank vars once, sorted", () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "compose-vars-test-"));
  const composeFile = path.join(dir, "compose.yaml");
  fs.writeFileSync(
    composeFile,
    [
      "services:",
      "  node1:",
      "    environment:",
      "      SEED_PHRASE: $UNSET_SEED",
      "      DB_URL: ${BLANK_VAR}",
      "      IMAGE: $SET_VAR",
      "      REPEAT: $UNSET_SEED",
    ].join("\n"),
  );

  const unset = collectUnsetComposeVars(composeFile, {
    SET_VAR: "value",
    BLANK_VAR: "",
  });

  assert.deepEqual(unset, ["BLANK_VAR", "UNSET_SEED"]);
});

test("run rejects --from-genesis combined with --from-snapshot", async () => {
  await assert.rejects(
    run("devnet", {
      fromGenesis: true,
      fromSnapshot: "https://example.com/snapshot.tgz",
    }),
    /mutually exclusive/,
  );
});

test("run rejects --compose-override without --from-genesis", async () => {
  await assert.rejects(
    run("devnet", { composeOverride: ["extra.override.yaml"] }),
    /--compose-override is only supported together with --from-genesis/,
  );
});
