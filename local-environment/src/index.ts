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

import { Command } from "commander";
import { run } from "./commands/run";
import { stop } from "./commands/stop";
import { imageUpgrade } from "./commands/imageUpgrade";
import { RunOptions, ImageUpgradeOptions } from "./lib/types";

const program = new Command();

// Local type for direct values received in Image Upgrade command
interface ImageUpgradeCliOpts {
  imageEnv?: string;
  include?: string;
  exclude?: string;
  profiles?: string[];
  envFile?: string[];
  waitBetween?: number;
  healthTimeout?: number;
  requireHealthy?: boolean;
}

program
  .command("run <network>")
  .option("-p, --profiles <profile...>", "Docker Compose profiles to activate")
  .option("--env-file <path...>", "specify one or more env files")
  .description(
    "Connect to Kubernetes, extract secrets, then run docker-compose up",
  )
  .action(async (network: string, options: RunOptions) => {
    await run(network, options);
  });

program
  .command("image-upgrade <network>")
  .option(
    "--image-env <VAR>",
    "Env var used in compose to pin image tag (default NODE_IMAGE)",
  )
  .option("--include <regex>", "Only roll services matching this regex")
  .option("--exclude <regex>", "Skip services matching this regex")
  .option("-p, --profiles <profile...>", "Docker Compose profiles to activate")
  .option("--env-file <path...>", "specify one or more env files")
  .option(
    "--wait-between <ms>",
    "Wait time between service upgrades in ms (default 5000)",
    parseInt,
  )
  .option(
    "--health-timeout <sec>",
    "Max seconds to wait for health per service (default 180)",
    parseInt,
  )
  .option(
    "--no-require-healthy",
    "Do not wait for healthchecks, just waitBetween",
  )
  .description(
    "Gradually roll out a new docker image tag across services in the given network",
  )
  .action(async (network: string, cliOpts: ImageUpgradeCliOpts) => {
    const profiles = cliOpts.profiles
      ?.map((s: string) => s.trim())
      .filter(Boolean);
    const opts: ImageUpgradeOptions = {
      imageEnvVar: cliOpts.imageEnv ?? "NODE_IMAGE",
      includePattern: cliOpts.include,
      excludePattern: cliOpts.exclude,
      profiles,
      envFile: cliOpts.envFile,
      waitBetweenMs: cliOpts.waitBetween ?? 5000,
      healthTimeoutSec: cliOpts.healthTimeout ?? 180,
      requireHealthy: cliOpts.requireHealthy !== false,
    };
    await imageUpgrade(network, opts);
  });

program
  .command("stop <network>")
  .option("-p, --profiles <profile...>", "Docker Compose profiles to activate")
  .description(
    "Stop the running docker-compose environment for the given network",
  )
  .action(async (network: string, options: RunOptions) => {
    await stop(network, options);
  });

// TODO: program commands for image upgrade, runtime, fork, etc...

program.parse();