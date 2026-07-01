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
import {
  discoverComposeDataMounts,
  restoreSnapshot,
} from "../lib/snapshotRestore";
import { loadNetworkConfig, requireMockConfig } from "../lib/networkConfig";
import {
  defaultMockAuthoritiesImage,
  runMockAuthoritiesConvert,
} from "../lib/mockAuthorities";
import {
  generateMockComposeOverride,
  mockOverridePath,
  MOCKED_CONFIG_DIRNAME,
} from "../lib/mockComposeOverride";

/**
 * Bring up a network locally:
 * - "local-env" runs the bundled local Cardano/PC stack from compose.
 * - Any well-known network (devnet/qanet/...) is forked from the
 *   provided snapshot via mock-authorities, or brought up from genesis
 *   with --from-genesis — there is no k8s-backed path.
 */
export async function run(network: string, runOptions: RunOptions) {
  if (network === "local-env") {
    console.log("Running environment with local Cardano/PC resources");
    await runLocalEnvironment(runOptions);
    return;
  }

  assertWellKnownNamespace(network);
  if (runOptions.fromGenesis) {
    console.log(`Bringing up ${network} from genesis (base compose, no fork)`);
  } else {
    console.log(
      `Preparing ${network} local fork (mock-authorities-driven bring-up)`,
    );
  }
  await runWellKnownNetwork(network, runOptions);
}

async function runWellKnownNetwork(namespace: string, runOptions: RunOptions) {
  if (runOptions.fromGenesis && runOptions.fromSnapshot) {
    throw new Error("--from-genesis and --from-snapshot are mutually exclusive.");
  }

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

  if (runOptions.fromGenesis) {
    await runFromGenesis(composeFile, env, runOptions);
    return;
  }

  const networkConfig = loadNetworkConfig(namespace);
  const mock = requireMockConfig(namespace, networkConfig);

  let overridePath: string;
  if (runOptions.fromSnapshot) {
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

    overridePath = generateMockComposeOverride({
      composeDir,
      network: namespace,
      validatorServices: mock.validatorServices,
      extraServices: mock.extraServices,
    });
    console.log(`Generated fork-mode override: ${overridePath}`);
  } else {
    // No snapshot: reuse the fork-mode artifacts from a previous bring-up.
    // Reuse only when both the generated mock-authorities output and the
    // restored data dirs are still present locally.
    overridePath = mockOverridePath(composeDir, namespace);
    assertReusableForkState(namespace, composeFile, composeDir, overridePath);
    console.log(`Reusing existing fork-mode override: ${overridePath}`);
  }

  await runDockerCompose({
    composeFile,
    extraComposeFiles: [overridePath],
    env,
    profiles: runOptions.profiles,
    detach: true,
  });
}

/**
 * Bring up the network's base compose from block 0 — no snapshot restore, no
 * mock-authorities rewrite. The base compose expects the real network inputs
 * (validator seed phrases, a main-chain data source) via env/--env-file, so
 * report anything left unset up front rather than letting the chain come up
 * silently unable to produce blocks.
 */
async function runFromGenesis(
  composeFile: string,
  env: Record<string, string>,
  runOptions: RunOptions,
) {
  const staleDataDirs = discoverComposeDataMounts(composeFile).filter((dir) =>
    isNonEmptyDirectory(dir),
  );
  if (staleDataDirs.length > 0) {
    console.warn(
      `⚠️  Existing chain data found (${staleDataDirs.join(", ")}); nodes will resume from it rather than genesis. Delete these directories first for a clean block-0 start.`,
    );
  }

  const unsetVars = collectUnsetComposeVars(composeFile, env);
  if (unsetVars.length > 0) {
    console.warn(
      `⚠️  Env vars referenced by the compose file but not set: ${unsetVars.join(", ")}. From-genesis mode mocks nothing — validator seeds and a main-chain data source must be provided via --env-file or the environment for the chain to produce blocks.`,
    );
  }

  await runDockerCompose({
    composeFile,
    env,
    profiles: runOptions.profiles,
    detach: true,
  });
}

/** Env var names referenced by the compose file ($VAR or ${VAR...}) that are unset or blank in env. */
function collectUnsetComposeVars(
  composeFile: string,
  env: Record<string, string>,
): string[] {
  const text = fs.readFileSync(composeFile, "utf8");
  const names = new Set<string>();
  for (const match of text.matchAll(/\$\{?([A-Z][A-Z0-9_]*)/g)) {
    names.add(match[1]);
  }
  return [...names].filter((name) => !env[name]).sort();
}

function assertReusableForkState(
  namespace: string,
  composeFile: string,
  composeDir: string,
  overridePath: string,
) {
  const requiredArtifacts = [
    overridePath,
    path.join(composeDir, MOCKED_CONFIG_DIRNAME, "mock-registrations.json"),
    path.join(composeDir, MOCKED_CONFIG_DIRNAME, "seeds"),
  ];
  const missingArtifacts = requiredArtifacts.filter((p) => !fs.existsSync(p));

  const missingDataDirs = discoverComposeDataMounts(composeFile).filter(
    (dir) => !isNonEmptyDirectory(dir),
  );

  if (missingArtifacts.length === 0 && missingDataDirs.length === 0) {
    return;
  }

  const problems: string[] = [];
  if (missingArtifacts.length > 0) {
    problems.push(
      `missing fork-mode artifacts: ${missingArtifacts.join(", ")}`,
    );
  }
  if (missingDataDirs.length > 0) {
    problems.push(
      `missing or empty restored data dirs: ${missingDataDirs.join(", ")}`,
    );
  }

  throw new Error(
    `--from-snapshot was not provided and reusable fork state for '${namespace}' is incomplete (${problems.join("; ")}). Provide --from-snapshot to perform the initial restore, or restore the snapshot data and re-run mock-authorities first.`,
  );
}

function isNonEmptyDirectory(dir: string): boolean {
  if (!fs.existsSync(dir)) {
    return false;
  }

  try {
    return fs.statSync(dir).isDirectory() && fs.readdirSync(dir).length > 0;
  } catch {
    return false;
  }
}

async function runLocalEnvironment(runOptions: RunOptions) {
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

  await runDockerCompose({
    composeFile,
    env,
    profiles: runOptions.profiles,
    detach: true,
  });
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
