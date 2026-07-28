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
import { existsSync } from "fs";
import { globSync } from "glob";

/**
 * Resolve the docker-compose file for a network. `local-env` uses its own
 * `docker-compose.yml`; every other name is a well-known network with a
 * `<name>.network.yaml` under `networks/well-known/<name>/`. Shared so the
 * upgrade commands (image-upgrade / full-upgrade) work on `local-env` too —
 * previously they only searched `well-known/` and threw for local-env.
 */
export function resolveComposeFile(network: string): string {
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
