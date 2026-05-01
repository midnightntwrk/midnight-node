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

import { existsSync } from "fs";
import path from "path";
import { globSync } from "glob";

import { NodeEndpoint, waitForFinality } from "../lib/waitForFinality";
import { discoverValidatorEndpoints } from "../lib/discoverValidators";

export interface VerifyFinalityOptions {
  targetBlock: number;
  timeoutMs: number;
  /**
   * Optional explicit endpoint list. If non-empty, replaces compose-file
   * discovery — useful for non-compose setups or for probing a remote node.
   */
  nodeOverrides?: NodeEndpoint[];
}

export async function verifyFinality(
  network: string | undefined,
  options: VerifyFinalityOptions,
): Promise<void> {
  const endpoints =
    options.nodeOverrides && options.nodeOverrides.length > 0
      ? options.nodeOverrides
      : discoverFromNetwork(network);

  await waitForFinality(endpoints, {
    targetBlock: options.targetBlock,
    timeoutMs: options.timeoutMs,
  });
}

function discoverFromNetwork(network: string | undefined): NodeEndpoint[] {
  if (!network) {
    throw new Error(
      "verify-finality requires either a <network> argument or one or more --node overrides",
    );
  }
  return discoverValidatorEndpoints(resolveComposeFile(network));
}

function resolveComposeFile(network: string): string {
  if (network === "local-env") {
    const composeFile = path.resolve(
      __dirname,
      "../networks/local-env/docker-compose.yml",
    );
    if (!existsSync(composeFile)) {
      throw new Error(`Compose file not found: ${composeFile}`);
    }
    return composeFile;
  }

  const searchPath = path.resolve(
    __dirname,
    "../networks",
    "well-known",
    network,
    "*.network.yaml",
  );
  const candidates = globSync(searchPath);
  if (candidates.length === 0) {
    throw new Error(
      `No compose file found for network '${network}' under well-known/`,
    );
  }
  const preferred = candidates.find(
    (p) => path.basename(p) === `${network}.network.yaml`,
  );
  return preferred ?? candidates[0];
}
