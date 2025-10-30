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
import { runtimeUpgrade } from "./commands/runtimeUpgrade";
import { snapshot } from "./commands/snapshot";
import {
  RunOptions,
  ImageUpgradeOptions,
  RuntimeUpgradeOptions,
  SnapshotOptions,
} from "./lib/types";

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
  fromSnapshot?: string;
}

interface RuntimeUpgradeCliOpts {
  wasm: string;
  rpcUrl?: string;
  sudoUri?: string;
  delayBlocks?: number;
  profiles?: string[];
  envFile?: string[];
  skipRun?: boolean;
  fromSnapshot?: string;
}

interface SnapshotCliOpts {
  bootnode?: string;
  pvc?: string;
  s3Uri?: string;
  snapshotImage?: string;
  timeout?: number;
}

program
  .command("run <network>")
  .option("-p, --profiles <profile...>", "Docker Compose profiles to activate")
  .option("--env-file <path...>", "specify one or more env files")
  .option(
    "--from-snapshot <id>",
    "Restore a bootnode snapshot before launching services",
  )
  .description(
    "Connect to Kubernetes, extract secrets, then run docker-compose up",
  )
  .action(async (network: string, options: RunOptions) => {
    await run(network, options);
  });

program
  .command("snapshot <network>")
  .option(
    "--bootnode <name>",
    "Name of the bootnode statefulset to snapshot (default midnight-node-boot-01)",
  )
  .option("--pvc <name>", "Explicit PVC name to mount when snapshotting")
  .option(
    "--s3-uri <uri>",
    "Destination S3 URI for the archived /node state (default MN_SNAPSHOT_S3_URI or s3://midnight-node-snapshots)",
  )
  .option(
    "--snapshot-image <image>",
    "Container image used to run the snapshot helper pod",
  )
  .option(
    "--timeout <minutes>",
    "Minutes to wait for the snapshot pod to finish (default 30)",
    parseInt,
  )
  .description(
    "Archive the /node volume from a bootnode PVC and upload it to the configured S3 destination",
  )
  .action(async (network: string, cliOpts: SnapshotCliOpts) => {
    const opts: SnapshotOptions = {
      bootnodeStatefulSet: cliOpts.bootnode,
      pvcName: cliOpts.pvc,
      s3Uri: cliOpts.s3Uri,
      snapshotImage: cliOpts.snapshotImage,
      timeoutMinutes: cliOpts.timeout,
    };

    await snapshot(network, opts);
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
  .option(
    "--from-snapshot <id>",
    "Restore a bootnode snapshot before launching the rollout",
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
      fromSnapshot: cliOpts.fromSnapshot,
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

program
  .command("runtime-upgrade <network>")
  .requiredOption("--wasm <path>", "Path to the runtime wasm blob")
  .option(
    "--skip-run",
    "Do not ensure docker-compose is running before upgrading",
  )
  .option(
    "--rpc-url <url>",
    "WebSocket RPC endpoint (default ws://localhost:9944)",
  )
  .option(
    "--sudo-uri <uri>",
    "Keyring URI used to submit the sudo upgrade (default env or //Alice)",
  )
  .option(
    "--delay-blocks <value>",
    "Blocks to wait from the current head before sending the upgrade",
    parseInt,
  )
  .option("-p, --profiles <profile...>", "Docker Compose profiles to activate")
  .option("--env-file <path...>", "specify one or more env files")
  .option(
    "--from-snapshot <id>",
    "Restore a bootnode snapshot before launching services",
  )
  .description("Submit a sudo runtime upgrade after an optional block delay")
  .action(async (network: string, cliOpts: RuntimeUpgradeCliOpts) => {
    const profiles = cliOpts.profiles
      ?.map((s: string) => s.trim())
      .filter(Boolean);

    const opts: RuntimeUpgradeOptions = {
      wasmPath: cliOpts.wasm,
      sudoUri: cliOpts.sudoUri,
      delayBlocks: cliOpts.delayBlocks,
      profiles,
      envFile: cliOpts.envFile,
      rpcUrl: cliOpts.rpcUrl,
      skipRun: cliOpts.skipRun,
      fromSnapshot: cliOpts.fromSnapshot,
    };

    await runtimeUpgrade(network, opts);
  });

program.parse();
