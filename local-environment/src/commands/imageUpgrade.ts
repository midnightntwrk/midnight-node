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
import fs, { existsSync } from "fs";
import { globSync } from "glob";
import { parse } from "dotenv";
import { spawn } from "child_process";
import { ImageUpgradeOptions } from "../lib/types";

// Command functionality we can depend on
import { run } from "./run";

function requireString(v: string | undefined, name: string): string {
  if (!v)
    throw new Error(`❌ ${name} is required. Pass a flag or set the env var.`);
  return v;
}

export async function imageUpgrade(
  namespace: string,
  opts: ImageUpgradeOptions,
) {
  const imageEnvVar = opts.imageEnvVar ?? "NODE_IMAGE";
  // The image we start from
  const fromTag = requireString(process.env.NODE_IMAGE, "NODE_IMAGE");
  // The image to upgrade/ rollout
  const toTag = requireString(process.env.NEW_NODE_IMAGE, "NEW_NODE_IMAGE");

  const waitBetweenMs = opts.waitBetweenMs ?? 5_000;
  const healthTimeoutSec = opts.healthTimeoutSec ?? 180;
  const requireHealthy = opts.requireHealthy ?? true;

  let env: Record<string, string> = {
    ...(process.env as Record<string, string>),
  };

  for (const envFilePath of opts.envFile ?? []) {
    if (fs.existsSync(envFilePath)) {
      const envOverrides = parse(fs.readFileSync(envFilePath));
      env = { ...env, ...envOverrides };
    } else {
      console.warn(`⚠️  Env file not found: ${envFilePath}`);
    }
  }

  console.log(`Ensuring network is up with starting tag ${fromTag}`);
  env[imageEnvVar] = fromTag;

  await run(namespace, {
    profiles: opts.profiles,
    envFile: opts.envFile,
    fromSnapshot: opts.fromSnapshot,
  });

  const composeFile = resolveNetworkCompose(namespace);
  const services = opts.services ?? (await listServices(composeFile, env));
  if (!services.length) {
    throw new Error(
      "No services discovered to roll out. Provide ImageUpgradeOptions.services explicitly or check your compose file.",
    );
  }

  console.log(`Rollout order: ${services.join(", ")}`);

  console.log(
    `Rolling services from tag ${fromTag} → ${toTag} via ${imageEnvVar}`,
  );
  for (const svc of services) {
    console.log(`\n Upgrading service: ${svc}`);
    env[imageEnvVar] = toTag;

    // Important: only re-create this one service, do not bounce dependencies
    await dockerCompose(["-f", composeFile, "up", "-d", "--no-deps", svc], {
      env,
      profiles: opts.profiles,
    });

    if (requireHealthy) {
      console.log(
        `Waiting for ${svc} to become healthy (timeout ${healthTimeoutSec}s)`,
      );
      await waitForHealthy(composeFile, svc, env, healthTimeoutSec);
    } else if (waitBetweenMs > 0) {
      await sleep(waitBetweenMs);
    }

    console.log(`${svc} upgraded.`);

    // revert the env back to fromTag so later `config` calls reflect the original image unless we deliberately change it again
    env[imageEnvVar] = fromTag;
    if (waitBetweenMs > 0) await sleep(waitBetweenMs);
  }

  console.log(
    `\n Rollout complete! All selected services are now on ${toTag}.`,
  );
}

function resolveNetworkCompose(namespace: string): string {
  const searchPath = path.resolve(
    __dirname,
    "../networks",
    "well-known",
    namespace,
    "*.network.yaml",
  );
  const candidates = globSync(searchPath);

  if (candidates.length === 0) {
    throw new Error(`No .network.yaml file found for namespace '${namespace}'`);
  }

  const preferred = candidates.find(
    (p) => path.basename(p) === `${namespace}.network.yaml`,
  );
  const composeFile = preferred || candidates[0];

  if (!existsSync(composeFile)) {
    throw new Error(`Resolved file not found: ${composeFile}`);
  }

  return composeFile;
}

async function listServices(
  composeFile: string,
  env: Record<string, string>,
): Promise<string[]> {
  const { code, stdout, stderr } = await sh(
    "docker",
    ["compose", "-f", composeFile, "config", "--services"],
    { env },
  );
  if (code !== 0) {
    console.warn(stderr);
    return [];
  }
  return stdout
    .split("\n")
    .map((s) => s.trim())
    .filter(Boolean);
}

async function waitForHealthy(
  composeFile: string,
  service: string,
  env: Record<string, string>,
  timeoutSec: number,
): Promise<void> {
  const deadline = Date.now() + timeoutSec * 1000;

  // We fetch container name(s) for the given service using `docker compose ps`.
  // Then we poll `.State.Health.Status` for each.
  while (Date.now() < deadline) {
    const names = await containerNamesForService(composeFile, service, env);
    if (!names.length) {
      // Service might still be starting. Keep waiting.
      await sleep(1500);
      continue;
    }

    let allHealthy = true;
    for (const name of names) {
      const status = await healthStatus(name, env);
      if (status === "healthy") continue;
      if (
        status === "starting" ||
        status === "unhealthy" ||
        status === "unknown"
      ) {
        allHealthy = false;
        break;
      }
      // If health is not defined, treat as healthy.
      if (status === null) continue;
    }

    if (allHealthy) return;
    await sleep(1500);
  }
  throw new Error(
    `Timeout waiting for ${service} to become healthy after ${timeoutSec}s`,
  );
}

async function containerNamesForService(
  composeFile: string,
  service: string,
  env: Record<string, string>,
): Promise<string[]> {
  const { code, stdout } = await sh(
    "docker",
    ["compose", "-f", composeFile, "ps", "--format", "{{.Name}}", service],
    { env },
  );
  if (code !== 0) return [];
  return stdout
    .split("\n")
    .map((s) => s.trim())
    .filter(Boolean);
}

async function healthStatus(
  containerName: string,
  env: Record<string, string>,
): Promise<"starting" | "healthy" | "unhealthy" | "unknown" | null> {
  const format =
    "{{ if .State.Health }}{{ .State.Health.Status }}{{ else }}NOHEALTH{{ end }}";
  const { code, stdout } = await sh(
    "docker",
    ["inspect", "--format", format, containerName],
    { env },
  );
  if (code !== 0) return "unknown";
  const out = stdout.trim();
  if (out === "NOHEALTH") return null;
  if (out === "healthy" || out === "starting" || out === "unhealthy")
    return out;
  return "unknown";
}

async function dockerCompose(
  args: string[],
  opts: { env: Record<string, string>; profiles?: string[] },
) {
  const passEnv: Record<string, string> = { ...opts.env };
  if (opts.profiles?.length) {
    // Docker Compose reads COMPOSE_PROFILES env var for default active profiles
    passEnv["COMPOSE_PROFILES"] = opts.profiles.join(",");
  }
  console.log(`$ docker ${["compose", ...args].join(" ")}`);
  const { code } = await sh("docker", ["compose", ...args], { env: passEnv });
  if (code !== 0) throw new Error(`docker compose ${args.join(" ")} failed`);
}

function sleep(ms: number) {
  return new Promise((res) => setTimeout(res, ms));
}

async function sh(
  cmd: string,
  args: string[],
  options: { env?: Record<string, string> } = {},
): Promise<{ code: number; stdout: string; stderr: string }> {
  return new Promise((resolve) => {
    const child = spawn(cmd, args, {
      stdio: ["ignore", "pipe", "pipe"],
      env: { ...process.env, ...(options.env ?? {}) },
    });
    let out = "",
      err = "";
    child.stdout.on("data", (d) => (out += d.toString()));
    child.stderr.on("data", (d) => (err += d.toString()));
    child.on("close", (code) =>
      resolve({ code: code ?? 0, stdout: out, stderr: err }),
    );
  });
}
