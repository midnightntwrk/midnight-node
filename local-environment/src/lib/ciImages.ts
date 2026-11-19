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

import { execFileSync } from "node:child_process";
import path from "node:path";
import { loadEnvDefault } from "./localEnv";

interface ComposeConfig {
  services: Record<string, { image?: string; profiles?: string[] }>;
}

const publicServices = {
  BUSYBOX_IMAGE: "busybox-init",
  CARDANO_IMAGE: "cardano-node-1",
  SOCAT_IMAGE: "cardano-node-socat",
  KUPO_IMAGE: "kupo",
  OGMIOS_IMAGE: "ogmios",
  DBSYNC_IMAGE: "db-sync",
  POSTGRES_IMAGE: "postgres",
  INDEXER_POSTGRES_IMAGE: "postgres-indexer",
  NATS_IMAGE: "nats",
};

export function ciImages(
  config: ComposeConfig,
  privateImages: string[],
): Record<string, string> {
  const images: Record<string, string> = {};
  for (const [arg, service] of Object.entries(publicServices)) {
    const image = config.services[service]?.image;
    if (!image)
      throw new Error(`CI dependency '${service}' has no resolved image.`);
    images[arg] = image;
  }
  const preloaded = new Set([...Object.values(images), ...privateImages]);
  for (const [service, configService] of Object.entries(config.services)) {
    if (
      configService.profiles?.length &&
      !configService.profiles.includes("withindexer")
    )
      continue;
    if (configService.image && !preloaded.has(configService.image)) {
      throw new Error(
        `CI service '${service}' is not preloaded: ${configService.image}`,
      );
    }
  }
  return images;
}

if (require.main === module) {
  const env = { ...loadEnvDefault(), ...process.env };
  env.MIDNIGHT_NODE_IMAGE = env.NODE_IMAGE;
  env.INDEXER_CHAIN_IMAGE = env.CHAIN_INDEXER_IMAGE;
  env.INDEXER_WALLET_IMAGE = env.WALLET_INDEXER_IMAGE;
  env.MIDNIGHT_RESERVE_CONTRACTS_PATH = path.resolve(
    "../midnight-reserve-contracts",
  );
  const privateImages = [
    env.NODE_IMAGE,
    env.TOOLKIT_IMAGE,
    env.INDEXER_API_IMAGE,
    env.CHAIN_INDEXER_IMAGE,
    env.WALLET_INDEXER_IMAGE,
  ];
  if (privateImages.some((image) => !image))
    throw new Error("CI needs all five private image references.");
  const output = execFileSync(
    "docker",
    [
      "compose",
      "--profile",
      "withindexer",
      "-f",
      path.join(__dirname, "../networks/local-env/docker-compose.yml"),
      "config",
      "--no-env-resolution",
      "--format",
      "json",
    ],
    { env, encoding: "utf8" },
  );
  process.stdout.write(
    JSON.stringify(ciImages(JSON.parse(output), privateImages as string[])),
  );
}
