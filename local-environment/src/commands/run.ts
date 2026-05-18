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
import fs, { existsSync } from "fs";
import { parse } from "dotenv";
import {
  generateSecretsIfMissing,
  getLocalEnvSecretVars,
  loadEnvDefault,
  requiredImageVars,
} from "../lib/localEnv";
import { assertWellKnownNamespace, RunOptions } from "../lib/types";
import { runDockerCompose } from "../lib/docker";
import { restoreSnapshot } from "../lib/snapshotRestore";
import { loadNetworkConfig, requireMockConfig } from "../lib/networkConfig";
import {
  defaultMockAuthoritiesImage,
  runMockAuthoritiesConvert,
} from "../lib/mockAuthorities";
import {
  generateMockComposeOverride,
  MOCKED_CONFIG_DIRNAME,
} from "../lib/mockComposeOverride";

/**
 * Bring up a network locally:
 * - "local-env" runs the bundled local Cardano/PC stack from compose.
 * - Any well-known network (devnet/qanet/...) is forked from the
 *   provided snapshot via mock-authorities — there is no k8s-backed path.
 */
export async function run(network: string, runOptions: RunOptions) {
  if (network === "local-env") {
    console.log("Running environment with local Cardano/PC resources");
    runLocalEnvironment(runOptions);
    return;
  }

  assertWellKnownNamespace(network);
  console.log(
    `Forking ${network} from snapshot (mock-authorities-driven bring-up)`,
  );
  await runWellKnownNetwork(network, runOptions);
}

async function runWellKnownNetwork(namespace: string, runOptions: RunOptions) {
  if (!runOptions.fromSnapshot) {
    throw new Error(
      "Forking a well-known network requires --from-snapshot (an http(s):// snapshot URI).",
    );
  }

  const networkConfig = loadNetworkConfig(namespace);
  const mock = requireMockConfig(namespace, networkConfig);

  const composeFile = resolveComposeFile(namespace);
  const composeDir = path.dirname(composeFile);

  let env: Record<string, string> = { ...cleanEnv(process.env) };
  for (const envFilePath of runOptions.envFile ?? []) {
    if (fs.existsSync(envFilePath)) {
      const envOverrides = parse(fs.readFileSync(envFilePath));
      env = { ...env, ...envOverrides };
    } else {
      console.warn(`⚠️  Env file not found: ${envFilePath}`);
    }
  }

  const restoredDirs = await restoreSnapshot({
    namespace,
    composeFile,
    snapshotUri: runOptions.fromSnapshot,
    env,
    permissive: true,
    // Snapshot tarballs are wrapped in a top-level `node/` dir; strip it so
    // chains/<chainId>/ lands directly at the data dir root, where both
    // mock-authorities and the node binary expect it.
    stripComponents: 1,
  });
  if (restoredDirs.length === 0) {
    throw new Error(
      `Snapshot restore produced no data dirs for '${namespace}'; cannot run mock-authorities convert.`,
    );
  }

  // mock-authorities expects --data-dir to be the parent containing every
  // per-validator subdir (data/node-1, data/node-2, ...) so it can patch each
  // one's paritydb with the synthesized authority set in a single pass.
  // Pointing it at one validator's dir leaves the others on the original
  // authority set and consensus never converges.
  const dataParentDir = path.dirname(restoredDirs[0]);
  const mockedConfigDir = path.join(composeDir, MOCKED_CONFIG_DIRNAME);
  runMockAuthoritiesConvert({
    dataDir: dataParentDir,
    outputDir: mockedConfigDir,
    chainId: mock.chainId,
    numValidators: mock.numValidators,
    image: defaultMockAuthoritiesImage(),
  });

  const overridePath = generateMockComposeOverride({
    composeDir,
    network: namespace,
    validatorServices: mock.validatorServices,
    extraServices: mock.extraServices,
  });
  console.log(`Generated fork-mode override: ${overridePath}`);

  runDockerCompose({
    composeFile,
    extraComposeFiles: [overridePath],
    env,
    profiles: runOptions.profiles,
    detach: true,
  });
}

function runLocalEnvironment(runOptions: RunOptions) {
  console.log("⚙️  Preparing local environment...");
  console.log(
    "ℹ️  Note: Midnight Governance will be active in 2 Cardano epochs.",
  );
  console.log("    The chain should start in 2 minutes.");

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
