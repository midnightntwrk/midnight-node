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

import { ensureImageAvailable, runDockerCompose } from "./docker";

const tmpDirs: string[] = [];

afterEach(() => {
  for (const dir of tmpDirs.splice(0)) {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

/**
 * Installs a fake `docker` executable on PATH so these tests never touch a
 * real daemon. Every invocation is appended (space-joined) to `logFile`.
 */
function fakeDockerEnv(opts: {
  pullExit?: number;
  inspectExit?: number;
  composeExit?: number;
  images?: string[];
}): { env: Record<string, string>; logFile: string } {
  const binDir = fs.mkdtempSync(path.join(os.tmpdir(), "fake-docker-bin-"));
  tmpDirs.push(binDir);
  const logFile = path.join(binDir, "calls.log");

  fs.writeFileSync(
    path.join(binDir, "docker"),
    `#!/usr/bin/env bash
echo "$@" >> "$FAKE_DOCKER_LOG"
if [ "$1" = "pull" ]; then
  exit "\${FAKE_DOCKER_PULL_EXIT:-0}"
fi
if [ "$1" = "image" ] && [ "$2" = "inspect" ]; then
  exit "\${FAKE_DOCKER_INSPECT_EXIT:-0}"
fi
if [ "$1" = "compose" ]; then
  for arg in "$@"; do
    if [ "$arg" = "--images" ]; then
      printf '%s\\n' "\${FAKE_DOCKER_IMAGES:-}"
      exit 0
    fi
  done
  exit "\${FAKE_DOCKER_COMPOSE_EXIT:-0}"
fi
exit 0
`,
  );
  fs.chmodSync(path.join(binDir, "docker"), 0o755);

  return {
    logFile,
    env: {
      PATH: `${binDir}:${process.env.PATH}`,
      FAKE_DOCKER_LOG: logFile,
      FAKE_DOCKER_PULL_EXIT: String(opts.pullExit ?? 0),
      FAKE_DOCKER_INSPECT_EXIT: String(opts.inspectExit ?? 0),
      FAKE_DOCKER_COMPOSE_EXIT: String(opts.composeExit ?? 0),
      FAKE_DOCKER_IMAGES: (opts.images ?? []).join("\n"),
    },
  };
}

describe("ensureImageAvailable", () => {
  it("succeeds without inspecting when the pull works", async () => {
    const { env, logFile } = fakeDockerEnv({ pullExit: 0 });
    await ensureImageAvailable("node:new", env);
    const calls = fs.readFileSync(logFile, "utf-8").trim().split("\n");
    assert.deepEqual(calls, ["pull node:new"]);
  });

  it("falls back to the local image when the pull fails but it exists locally", async () => {
    const { env, logFile } = fakeDockerEnv({ pullExit: 1, inspectExit: 0 });
    await ensureImageAvailable("local-only:dev", env);
    const calls = fs.readFileSync(logFile, "utf-8").trim().split("\n");
    assert.deepEqual(calls, ["pull local-only:dev", "image inspect local-only:dev"]);
  });

  it("fails when the image is in neither place", async () => {
    const { env } = fakeDockerEnv({ pullExit: 1, inspectExit: 1 });
    await assert.rejects(
      () => ensureImageAvailable("ghost:missing", env),
      /was not found locally or in a registry/,
    );
  });
});

describe("runDockerCompose", () => {
  it("ensures every image resolved by the compose config before starting", async () => {
    const { env, logFile } = fakeDockerEnv({
      images: ["ghcr.io/midnight-ntwrk/midnight-node:new", "postgres:17.1-alpine"],
    });
    await runDockerCompose({ composeFile: "docker-compose.yml", env });
    const calls = fs.readFileSync(logFile, "utf-8").trim().split("\n");
    assert.ok(calls.some((c) => c.startsWith("compose") && c.includes("--images")));
    assert.ok(calls.includes("pull ghcr.io/midnight-ntwrk/midnight-node:new"));
    assert.ok(calls.includes("pull postgres:17.1-alpine"));
    assert.ok(calls.some((c) => c.includes(" up --build")));
  });

  it("fails the bring-up if one of the resolved images is unavailable anywhere", async () => {
    const { env } = fakeDockerEnv({
      pullExit: 1,
      inspectExit: 1,
      images: ["ghost:missing"],
    });
    await assert.rejects(
      () => runDockerCompose({ composeFile: "docker-compose.yml", env }),
      /was not found locally or in a registry/,
    );
  });
});
