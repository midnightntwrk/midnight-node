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

import { spawn } from "child_process";

// TODO: Replace with docker library

export interface DockerComposeOptions {
  composeFile: string;
  /** Additional compose files layered on top via repeated `-f`. Order matters: later files override earlier. */
  extraComposeFiles?: string[];
  env: Record<string, string>;
  profiles?: string[];
  detach?: boolean;
}

function fileArgs(options: DockerComposeOptions): string[] {
  const args = ["-f", options.composeFile];
  for (const extra of options.extraComposeFiles ?? []) {
    args.push("-f", extra);
  }
  return args;
}

export function stopDockerCompose(options: DockerComposeOptions) {
  const args = [...fileArgs(options), "down", "--volumes", "--timeout", "0"];

  if (options.profiles) {
    for (const profile of options.profiles) {
      args.unshift(`--profile=${profile}`);
    }
  }
  args.unshift("compose");

  const docker = spawn("docker", args, {
    stdio: "inherit",
    env: options.env,
  });

  docker.on("exit", (code) => {
    if (code !== 0) {
      console.error(`❌ docker-compose down failed`);
      process.exit(code ?? 1);
    }
  });
}

function spawnDockerCompose(args: string[], env: Record<string, string>, onFail: (code: number | null) => Error): Promise<void> {
  return new Promise((resolve, reject) => {
    const docker = spawn("docker", args, { stdio: "inherit", env });
    docker.on("error", reject);
    docker.on("exit", (code) => {
      if (code === 0) {
        resolve();
        return;
      }
      reject(onFail(code));
    });
  });
}

function runDocker(args: string[], env: Record<string, string>): Promise<number> {
  return new Promise((resolve, reject) => {
    const docker = spawn("docker", args, { stdio: "inherit", env });
    docker.on("error", reject);
    docker.on("exit", (code) => resolve(code ?? 1));
  });
}

/**
 * Make sure `image` is available and up to date: pull it so a moved tag isn't
 * served stale, but if the pull fails (no such repository, offline, ...) fall
 * back to a local copy rather than pulling. Only fails if the image is in
 * neither place.
 */
export async function ensureImageAvailable(
  image: string,
  env: Record<string, string>,
): Promise<void> {
  if ((await runDocker(["pull", image], env)) === 0) return;

  const inspectCode = await new Promise<number>((resolve, reject) => {
    const docker = spawn("docker", ["image", "inspect", image], {
      stdio: "ignore",
      env,
    });
    docker.on("error", reject);
    docker.on("exit", (code) => resolve(code ?? 1));
  });
  if (inspectCode !== 0) {
    throw new Error(`Image '${image}' was not found locally or in a registry.`);
  }
  console.warn(`⚠️  Could not pull ${image}; using the local copy.`);
}

function resolveComposeImages(options: DockerComposeOptions): Promise<string[]> {
  const args = [
    "compose",
    ...(options.profiles ?? []).map((p) => `--profile=${p}`),
    ...fileArgs(options),
    "config",
    "--format",
    "json",
  ];
  return new Promise((resolve, reject) => {
    const docker = spawn("docker", args, {
      stdio: ["ignore", "pipe", "pipe"],
      env: options.env,
    });
    let stdout = "";
    let stderr = "";
    docker.stdout.on("data", (chunk) => (stdout += chunk));
    docker.stderr.on("data", (chunk) => (stderr += chunk));
    docker.on("error", reject);
    docker.on("exit", (code) => {
      if (code !== 0) {
        reject(
          new Error(
            `docker compose config --format json failed: ${stderr.trim()}`,
          ),
        );
        return;
      }
      let config: { services?: Record<string, { image?: string }> };
      try {
        config = JSON.parse(stdout);
      } catch (err) {
        reject(
          new Error(
            `docker compose config --format json produced invalid JSON: ${err}`,
          ),
        );
        return;
      }
      resolve(
        Object.values(config.services ?? {})
          .map((service) => service.image)
          .filter((image): image is string => Boolean(image)),
      );
    });
  });
}

export async function runDockerCompose(options: DockerComposeOptions): Promise<void> {
  const images = new Set(await resolveComposeImages(options));
  for (const image of images) {
    await ensureImageAvailable(image, options.env);
  }

  const profileArgs = (options.profiles ?? []).map((p) => `--profile=${p}`);
  const args = ["compose", ...profileArgs, ...fileArgs(options), "up", "--build"];
  if (options.detach) {
    args.push("--detach");
  }

  await spawnDockerCompose(
    args,
    options.env,
    (code) => new Error(`docker compose up exited with code ${code}`),
  );
}

/**
 * Stop and remove selected services while preserving their named/bind-mounted
 * data. Used when a regenerated fork reduces its active validator count so
 * containers from the previous topology cannot keep participating.
 */
export function removeDockerComposeServices(
  options: DockerComposeOptions,
  services: string[],
): Promise<void> {
  if (services.length === 0) return Promise.resolve();

  const args = [...fileArgs(options), "rm", "--stop", "--force", ...services];
  if (options.profiles) {
    for (const profile of options.profiles) {
      args.unshift(`--profile=${profile}`);
    }
  }
  args.unshift("compose");

  return new Promise((resolve, reject) => {
    const docker = spawn("docker", args, {
      stdio: "inherit",
      env: options.env,
    });

    docker.on("error", reject);
    docker.on("exit", (code) => {
      if (code === 0) {
        resolve();
        return;
      }
      reject(
        new Error(
          `docker compose rm exited with code ${code} for services: ${services.join(", ")}`,
        ),
      );
    });
  });
}
