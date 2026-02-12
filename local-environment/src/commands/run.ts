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
import fs, { existsSync } from "fs";
import { parse } from "dotenv";
import { connectToPostgres } from "../lib/connectToPostgres";
import { getSecrets } from "../lib/getSecretsForEnv";
import { prepareNamespaceKeystore } from "../lib/keystore";
import {
  generateSecretsIfMissing,
  getLocalEnvSecretVars,
  loadEnvDefault,
  requiredImageVars,
} from "../lib/localEnv";
import { assertWellKnownNamespace, RunOptions } from "../lib/types";
import { runDockerCompose } from "../lib/docker";
import { restoreSnapshotFromS3 } from "../lib/snapshotRestore";
import { ensureSnapshotCredentials } from "../lib/snapshotEnv";
import { setupPreviewProxies } from "../lib/previewProxy";
import { loadNetworkConfig } from "../lib/networkConfig";

/**
 * Runs a specified network, with passed configuration
 *
 * @param network - The name of the network to run, or else `local-env`
 */
export async function run(network: string, runOptions: RunOptions) {
  // TODO: For now, we will run the local environment as a separate option. In the future, we will include it as an option to run local env pc resources, alongside midnight nodes of the chosen environment
  if (network === "local-env") {
    console.log("Running environment with local Cardano/PC resources");
    runLocalEnvironment(runOptions);
  } else {
    assertWellKnownNamespace(network);
    console.log(
      `Running ${network} chain from genesis with ${network} Cardano/PC resources`,
    );
    await runEphemeralEnvironment(network, runOptions);
  }
}

async function runEphemeralEnvironment(
  namespace: string,
  runOptions: RunOptions,
) {
  const networkConfig = loadNetworkConfig(namespace);
  const dbsyncMode = networkConfig.dbsync.mode;

  switch(dbsyncMode) {
  case "public":
    console.log("Skipping port-forward: DB marked as publicly reachable");
  case "rds-proxy":
    console.log("Skipping pod port-forward: DB will be proxied via RDS helper");
  default:
    await connectToPostgres(namespace);
  }

  console.log(`🔐 Extracting secrets for namespace: ${namespace}`);
  const envObject = getSecrets(namespace);

  let env: Record<string, string> = { ...cleanEnv(process.env), ...envObject };

  for (const envFilePath of runOptions.envFile ?? []) {
    if (fs.existsSync(envFilePath)) {
      const envOverrides = parse(fs.readFileSync(envFilePath));
      env = { ...env, ...envOverrides };
    } else {
      console.warn(`⚠️  Env file not found: ${envFilePath}`);
    }
  }

  if (dbsyncMode === "rds-proxy") {
    const proxyOverrides = await setupPreviewProxies(env, namespace);
    env = { ...env, ...proxyOverrides };
  }

  const composeFile = resolveComposeFile(namespace);

  if (runOptions.fromSnapshot) {
    ensureSnapshotCredentials(env);
    await restoreSnapshotFromS3({
      namespace,
      composeFile,
      snapshotId: runOptions.fromSnapshot,
      env,
    });
  }

  // Setup keystore on nodes
  await prepareNamespaceKeystore({
    namespace,
    env,
  });

  runDockerCompose({
    composeFile,
    env,
    profiles: runOptions.profiles,
    detach: true,
  });
}

function runLocalEnvironment(runOptions: RunOptions) {
  console.log("⚙️  Preparing local environment...");
  console.log(
    "ℹ️  Note: The midnight-setup container will wait for the Cardano devnet to reach epoch 4.",
  );
  console.log("   This typically takes 2-5 minutes on a fresh start.");

  if (runOptions.fromSnapshot) {
    console.warn(
      "--from-snapshot is not supported for the local-env target; ignoring.",
    );
  }

  generateSecretsIfMissing();

  const localEnvSecretVars = getLocalEnvSecretVars();
  const envDefault = loadEnvDefault();

  let env: Record<string, string> = {
    ...envDefault,
    ...localEnvSecretVars,
  };

  for (const envFilePath of runOptions.envFile ?? []) {
    if (fs.existsSync(envFilePath)) {
      const envOverrides = parse(fs.readFileSync(envFilePath));
      env = { ...env, ...envOverrides };
    } else {
      console.warn(`⚠️  Env file not found: ${envFilePath}`);
    }
  }

  // Process environment variables take precendence
  env = {
    ...env,
    ...cleanEnv(process.env),
  };

  const missing = requiredImageVars.filter((key) => !env[key]);
  if (missing.length > 0) {
    console.error(`❌ Missing required image env vars: ${missing.join(", ")}`);
    process.exit(1);
  }

  const composeFile = path.resolve(
    __dirname,
    "../networks/local-env/docker-compose.yml",
  );

  runDockerCompose({
    composeFile,
    env,
    profiles: runOptions.profiles,
    detach: true,
  });

  return;
}

function resolveComposeFile(namespace: string): string {
  const searchPath = path.resolve(
    __dirname,
    "../networks",
    "well-known",
    namespace,
    "*.network.yaml",
  );
  const candidates = globSync(searchPath);

  if (candidates.length === 0) {
    console.error(`No .network.yaml file found for namespace '${namespace}'`);
    process.exit(1);
  }

  const preferred = candidates.find(
    (p) => path.basename(p) === `${namespace}.network.yaml`,
  );
  const composeFile = preferred || candidates[0];

  if (!existsSync(composeFile)) {
    console.error(`Resolved file not found: ${composeFile}`);
    process.exit(1);
  }

  return composeFile;
}

// Helper to ensure no undefined values in env vars
function cleanEnv(
  env: Record<string, string | undefined>,
): Record<string, string> {
  return Object.fromEntries(
    Object.entries(env).filter(([, v]) => typeof v === "string"),
  ) as Record<string, string>;
}
