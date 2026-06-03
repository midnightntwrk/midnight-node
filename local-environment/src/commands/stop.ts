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

import path from "path";
import { globSync } from "glob";
import { existsSync, rmSync } from "fs";

import { RunOptions } from "../lib/types";
import { stopDockerCompose } from "../lib/docker";
import {
  generateSecretsIfMissing,
  getLocalEnvSecretVars,
  loadEnvDefault,
  requiredImageVars,
} from "../lib/localEnv";
import { currentLayout, layoutEnv } from "../lib/ports";

export async function stop(network: string, runOptions: RunOptions) {
  if (network === "local-env") {
    stopLocalEnvironment(runOptions);
    return;
  }
  console.log(`Stop ${network} chain`);
  stopWellKnownNetwork(network, runOptions);
}

function stopWellKnownNetwork(namespace: string, runOptions: RunOptions) {
  const searchPath = path.resolve(
    __dirname,
    "../networks",
    "well-known",
    namespace,
    "*.network.yaml",
  );
  const candidates = globSync(searchPath);

  if (candidates.length === 0) {
    console.error(
      `❌ No .network.yaml file found for namespace '${namespace}'`,
    );
    process.exit(1);
  }

  const preferred = candidates.find(
    (p) => path.basename(p) === `${namespace}.network.yaml`,
  );
  const composeFile = preferred || candidates[0];

  if (!existsSync(composeFile)) {
    console.error(`❌ Resolved file not found: ${composeFile}`);
    process.exit(1);
  }

  stopDockerCompose({
    composeFile,
    env: cleanEnv(process.env),
    profiles: runOptions.profiles,
  });
}

function stopLocalEnvironment(runOptions: RunOptions) {
  console.log("⚙️  Preparing local environment...");

  generateSecretsIfMissing();

  const localEnvSecretVars = getLocalEnvSecretVars();
  const envDefault = loadEnvDefault();
  // Match the layout used at bring-up so `down` targets this slot's compose
  // project (and its volumes), not a sibling runner's stack.
  const layout = currentLayout();
  const finalEnv: Record<string, string> = {
    ...envDefault,
    ...localEnvSecretVars,
    ...cleanEnv(process.env),
    ...layoutEnv(layout),
  };

  const missing = requiredImageVars.filter((key) => !finalEnv[key]);
  if (missing.length > 0) {
    console.error(`❌ Missing required image env vars: ${missing.join(", ")}`);
    process.exit(1);
  }

  const composeFile = path.resolve(
    __dirname,
    "../networks/local-env/docker-compose.yml",
  );
  stopDockerCompose({
    composeFile,
    env: finalEnv,
    profiles: runOptions.profiles,
  });

  // Clean up runtime-values folder (bind mount not removed by docker compose down --volumes)
  const runtimeValuesPath = path.resolve(
    __dirname,
    "../networks/local-env/runtime-values",
  );
  if (existsSync(runtimeValuesPath)) {
    rmSync(runtimeValuesPath, { recursive: true, force: true });
    console.log("🧹 Cleaned up runtime-values");
  }
}

// Helper to ensure no undefined values in env vars
function cleanEnv(
  env: Record<string, string | undefined>,
): Record<string, string> {
  return Object.fromEntries(
    Object.entries(env).filter(([, v]) => typeof v === "string"),
  ) as Record<string, string>;
}
