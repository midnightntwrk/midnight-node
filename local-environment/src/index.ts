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

import { Command } from "commander";
import { run } from "./commands/run";
import { stop } from "./commands/stop";
import { imageUpgrade } from "./commands/imageUpgrade";
import { federatedRuntimeUpgrade } from "./commands/federatedRuntimeUpgrade";
import { fullUpgrade } from "./commands/fullUpgrade";
import { verifyFinality } from "./commands/verifyFinality";
import {
  RunOptions,
  ImageUpgradeOptions,
  FederatedRuntimeUpgradeOptions,
  FullUpgradeOptions,
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
  waitBefore?: number;
}

interface FederatedRuntimeUpgradeCliOpts {
  wasm: string;
  rpcUrl?: string;
  councilUris: string[];
  technicalUris: string[];
  executorUri: string;
  profiles?: string[];
  envFile?: string[];
  skipRun?: boolean;
  fromSnapshot?: string;
  allowSameVersion?: boolean;
}

interface FullUpgradeCliOpts {
  // image-upgrade surface
  imageEnv?: string;
  include?: string;
  exclude?: string;
  waitBetween?: number;
  waitBefore?: number;
  healthTimeout?: number;
  requireHealthy?: boolean;
  // governance runtime upgrade surface
  wasm: string;
  rpcUrl?: string;
  councilUris: string[];
  technicalUris: string[];
  executorUri: string;
  allowSameVersion?: boolean;
  // shared
  profiles?: string[];
  envFile?: string[];
  fromSnapshot?: string;
}

program
  .command("run <network>")
  .option("-p, --profiles <profile...>", "Docker Compose profiles to activate")
  .option("--env-file <path...>", "specify one or more env files")
  .option(
    "--from-snapshot <uri>",
    "http(s):// snapshot URI to restore before the first well-known-network bring-up. Later runs can omit it to reuse existing local fork state.",
  )
  .description(
    "Bring up a forked well-known network from a snapshot using mock-authorities, reuse an existing local fork, or run the local-env target.",
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
    "--wait-before <ms>",
    "Wait time before starting any service upgrades in ms (default 30000)",
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
    "--from-snapshot <uri>",
    "http(s):// snapshot URI to fork the network from before rolling the image",
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
      waitBeforeMs: cliOpts.waitBefore,
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
  .command("verify-finality [network]")
  .option(
    "-b, --target-block <number>",
    "Wait until every node has finalized at least this block number",
    "1",
  )
  .option(
    "-t, --timeout <seconds>",
    "Maximum seconds to wait before failing",
    "300",
  )
  .option(
    "-n, --node <name=url>",
    "Override compose discovery: probe the given name=url endpoint(s). Repeatable.",
    (value: string, prev: string[] = []) => [...prev, value],
    [] as string[],
  )
  .description(
    "Wait for every validator to finalize a block — fails if GRANDPA stalls. " +
      "Validators are auto-discovered from the named network's compose file " +
      "(services labeled io.midnight.role=validator). Pass --node to override discovery.",
  )
  .action(
    async (
      network: string | undefined,
      cliOpts: { targetBlock: string; timeout: string; node: string[] },
    ) => {
      const targetBlock = Number.parseInt(cliOpts.targetBlock, 10);
      const timeoutSec = Number.parseInt(cliOpts.timeout, 10);
      if (!Number.isFinite(targetBlock) || targetBlock < 0) {
        throw new Error(`Invalid --target-block: ${cliOpts.targetBlock}`);
      }
      if (!Number.isFinite(timeoutSec) || timeoutSec <= 0) {
        throw new Error(`Invalid --timeout: ${cliOpts.timeout}`);
      }
      const nodeOverrides = cliOpts.node.map((spec) => {
        const idx = spec.indexOf("=");
        if (idx <= 0 || idx === spec.length - 1) {
          throw new Error(
            `Invalid --node spec '${spec}': expected name=url (e.g. node-1=http://localhost:9933)`,
          );
        }
        return { name: spec.slice(0, idx), url: spec.slice(idx + 1) };
      });
      await verifyFinality(network, {
        targetBlock,
        timeoutMs: timeoutSec * 1_000,
        nodeOverrides: nodeOverrides.length > 0 ? nodeOverrides : undefined,
      });
    },
  );

program
  .command("governance-runtime-upgrade <network>")
  .requiredOption("--wasm <path>", "Path to the runtime wasm blob")
  .requiredOption(
    "--council-uris <uri...>",
    "Space-separated sr25519 URIs for council proposers and voters (must meet the 2/3 threshold)",
  )
  .requiredOption(
    "--technical-uris <uri...>",
    "Space-separated sr25519 URIs for technical committee proposers and voters (must meet the 2/3 threshold)",
  )
  .requiredOption(
    "--executor-uri <uri>",
    "Key URI used to close the federated motion and apply the authorized upgrade",
  )
  .option(
    "--rpc-url <url>",
    "WebSocket RPC endpoint (default ws://localhost:9944)",
  )
  .option(
    "--skip-run",
    "Do not ensure docker-compose is running before upgrading",
  )
  .option("-p, --profiles <profile...>", "Docker Compose profiles to activate")
  .option("--env-file <path...>", "specify one or more env files")
  .option(
    "--from-snapshot <uri>",
    "Restore an http(s) snapshot before launching services. Omit it to reuse existing local fork state.",
  )
  .option(
    "--allow-same-version",
    "Use system.authorizeUpgradeWithoutChecks so the upgrade is accepted even if the candidate wasm shares spec_version with the running runtime. Local-rehearsal escape hatch; do not use against production-shaped networks.",
  )
  .description(
    "Execute a governance-approved runtime upgrade using the federated-authority pallet",
  )
  .action(async (network: string, cliOpts: FederatedRuntimeUpgradeCliOpts) => {
    const profiles = cliOpts.profiles
      ?.map((s: string) => s.trim())
      .filter(Boolean);
    const councilUris = (cliOpts.councilUris || [])
      .map((uri: string) => uri.trim())
      .filter(Boolean);
    const techUris = (cliOpts.technicalUris || [])
      .map((uri: string) => uri.trim())
      .filter(Boolean);
    const executorUri = cliOpts.executorUri?.trim();

    if (!councilUris.length) {
      throw new Error("At least one council URI is required.");
    }
    if (!techUris.length) {
      throw new Error("At least one technical committee URI is required.");
    }
    if (!executorUri) {
      throw new Error("executor-uri is required and cannot be empty");
    }

    const opts: FederatedRuntimeUpgradeOptions = {
      wasmPath: cliOpts.wasm,
      rpcUrl: cliOpts.rpcUrl,
      skipRun: cliOpts.skipRun,
      profiles,
      envFile: cliOpts.envFile,
      fromSnapshot: cliOpts.fromSnapshot,
      councilUris,
      techCommitteeUris: techUris,
      motionExecutorUri: executorUri,
      allowSameVersion: cliOpts.allowSameVersion,
    };

    await federatedRuntimeUpgrade(network, opts);
  });

program
  .command("full-upgrade <network>")
  .requiredOption("--wasm <path>", "Path to the runtime wasm blob")
  .requiredOption(
    "--council-uris <uri...>",
    "Space-separated sr25519 URIs for council proposers and voters (must meet the 2/3 threshold)",
  )
  .requiredOption(
    "--technical-uris <uri...>",
    "Space-separated sr25519 URIs for technical committee proposers and voters (must meet the 2/3 threshold)",
  )
  .requiredOption(
    "--executor-uri <uri>",
    "Key URI used to close the federated motion and apply the authorized upgrade",
  )
  .option(
    "--rpc-url <url>",
    "WebSocket RPC endpoint for the runtime upgrade phase (default ws://localhost:9944)",
  )
  .option(
    "--image-env <VAR>",
    "Env var used in compose to pin image tag (default NODE_IMAGE)",
  )
  .option("--include <regex>", "Only roll services matching this regex")
  .option("--exclude <regex>", "Skip services matching this regex")
  .option(
    "--wait-between <ms>",
    "Wait time between service upgrades in ms (default 5000)",
    parseInt,
  )
  .option(
    "--wait-before <ms>",
    "Wait time before starting any service upgrades in ms (default 30000)",
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
  .option("-p, --profiles <profile...>", "Docker Compose profiles to activate")
  .option("--env-file <path...>", "specify one or more env files")
  .option(
    "--from-snapshot <uri>",
    "http(s):// snapshot URI to restore before phase 1. Required for the first bring-up of a well-known network.",
  )
  .option(
    "--allow-same-version",
    "Use system.authorizeUpgradeWithoutChecks in phase 2 so the upgrade is accepted even if the candidate wasm shares spec_version with the running runtime. Local-rehearsal escape hatch; do not use against production-shaped networks.",
  )
  .description(
    "Run a two-phase upgrade rehearsal: roll the validator client image (phase 1), then submit a governance-approved runtime upgrade (phase 2)",
  )
  .action(async (network: string, cliOpts: FullUpgradeCliOpts) => {
    const profiles = cliOpts.profiles
      ?.map((s: string) => s.trim())
      .filter(Boolean);
    const councilUris = (cliOpts.councilUris || [])
      .map((uri: string) => uri.trim())
      .filter(Boolean);
    const techUris = (cliOpts.technicalUris || [])
      .map((uri: string) => uri.trim())
      .filter(Boolean);
    const executorUri = cliOpts.executorUri?.trim();

    if (!councilUris.length) {
      throw new Error("At least one council URI is required.");
    }
    if (!techUris.length) {
      throw new Error("At least one technical committee URI is required.");
    }
    if (!executorUri) {
      throw new Error("executor-uri is required and cannot be empty");
    }

    const opts: FullUpgradeOptions = {
      // image-upgrade surface
      imageEnvVar: cliOpts.imageEnv ?? "NODE_IMAGE",
      includePattern: cliOpts.include,
      excludePattern: cliOpts.exclude,
      waitBeforeMs: cliOpts.waitBefore,
      waitBetweenMs: cliOpts.waitBetween ?? 5000,
      healthTimeoutSec: cliOpts.healthTimeout ?? 180,
      requireHealthy: cliOpts.requireHealthy !== false,
      // runtime upgrade surface
      wasmPath: cliOpts.wasm,
      rpcUrl: cliOpts.rpcUrl,
      councilUris,
      techCommitteeUris: techUris,
      motionExecutorUri: executorUri,
      allowSameVersion: cliOpts.allowSameVersion,
      // shared
      profiles,
      envFile: cliOpts.envFile,
      fromSnapshot: cliOpts.fromSnapshot,
    };

    await fullUpgrade(network, opts);
  });

program.parse();
