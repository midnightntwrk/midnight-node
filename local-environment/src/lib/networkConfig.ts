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

export interface MockValidatorSelection {
  numValidators: number;
  validatorServices: string[];
  disabledValidatorServices: string[];
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

/**
 * Resolve the active validator topology for a fork. The configured service
 * list is the maximum topology that Docker Compose can materialize; a CLI
 * override may select a prefix of that list but cannot invent new services.
 */
export function resolveMockValidatorSelection(
  config: MockModeConfig,
  requestedNumValidators?: number,
): MockValidatorSelection {
  const numValidators = requestedNumValidators ?? config.numValidators;
  const source =
    requestedNumValidators === undefined
      ? "mock.numValidators in config.json"
      : "--num-validators";

  if (!Number.isSafeInteger(numValidators) || numValidators < 1) {
    throw new Error(
      `${source} must be a positive integer; got ${numValidators}`,
    );
  }

  if (numValidators > config.validatorServices.length) {
    throw new Error(
      `${source} requested ${numValidators} validators, but this network's Compose topology only defines ${config.validatorServices.length}: ${config.validatorServices.join(", ")}`,
    );
  }

  return {
    numValidators,
    validatorServices: config.validatorServices.slice(0, numValidators),
    disabledValidatorServices: config.validatorServices.slice(numValidators),
  };
}
