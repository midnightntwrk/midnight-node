// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
// http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

import fs from "fs";
import path from "path";

export interface MockModeConfig {
  /** Substrate chain id (matches the on-disk paritydb chain folder name in the snapshot). */
  chainId: string;
  /** Number of validators to materialize with mock-authorities. */
  numValidators: number;
  /** Compose service names that map ./data/<svc>:/data and need seeds mounted. */
  validatorServices: string[];
  /** Non-validator services that still need fork-mode env (e.g. qanet's boot-node). */
  extraServices?: string[];
}

export interface NetworkConfig {
  mock?: MockModeConfig;
}

export function loadNetworkConfig(namespace: string): NetworkConfig {
  const configPath = path.resolve(
    __dirname,
    "../networks",
    "well-known",
    namespace,
    "config.json",
  );

  if (!fs.existsSync(configPath)) {
    return {};
  }

  try {
    const raw = fs.readFileSync(configPath, "utf-8");
    return JSON.parse(raw) as NetworkConfig;
  } catch (error) {
    throw new Error(
      `Failed to parse network config at ${configPath}: ${(error as Error).message}`,
    );
  }
}

export function requireMockConfig(
  namespace: string,
  config: NetworkConfig,
): MockModeConfig {
  if (!config.mock) {
    throw new Error(
      `Network '${namespace}' has no 'mock' section in config.json — fork bring-up is unsupported for this network.`,
    );
  }
  return config.mock;
}
