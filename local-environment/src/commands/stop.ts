// This file is part of midnight-node.
// Copyright (C) 2025 Midnight Foundation
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
import { existsSync } from "fs";

import { RunOptions } from "../lib/types";
import { stopDockerCompose } from "../lib/docker";
import { connectToPostgres } from "../lib/connectToPostgres";
import { getSecrets } from "../lib/getSecretsForEnv";
import {
  generateSecretsIfMissing,
  getLocalEnvSecretVars,
  loadEnvDefault,
  requiredImageVars,
} from "../lib/localEnv";

export async function stop(network: string, runOptions: RunOptions) {
  // TODO: For now, we will run the local environment as a separate option. In the future, we will include it as an option to run local env pc resources, alongside midnight nodes of the chosen environment
  if (network === "local-env") {
    console.log("Running environment with local Cardano/PC resources");
    stopLocalEnvironment(runOptions);
  } else {
    console.log(`Stop ${network} chain`);
    stopEphemeralEnvironment(network, runOptions);
  }
}

async function stopEphemeralEnvironment(
  namespace: string,
  runOptions: RunOptions,
) {
  console.log(`🔌 Connecting to Kubernetes pods for namespace: ${namespace}`);
  await connectToPostgres(namespace);

  console.log(`🔐 Extracting secrets for namespace: ${namespace}`);
  const envObject = getSecrets(namespace);

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

  // Prefer: <namespace>.network.yaml
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
    env: { ...cleanEnv(process.env), ...envObject },
    profiles: runOptions.profiles,
  });
}

function stopLocalEnvironment(runOptions: RunOptions) {
  console.log("⚙️  Preparing local environment...");

  generateSecretsIfMissing();

  const localEnvSecretVars = getLocalEnvSecretVars();
  const envDefault = loadEnvDefault();
  const finalEnv: Record<string, string> = {
    ...envDefault,
    ...localEnvSecretVars,
    ...cleanEnv(process.env),
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

  return;
}

// Helper to ensure no undefined values in env vars
function cleanEnv(
  env: Record<string, string | undefined>,
): Record<string, string> {
  return Object.fromEntries(
    Object.entries(env).filter(([, v]) => typeof v === "string"),
  ) as Record<string, string>;
}
