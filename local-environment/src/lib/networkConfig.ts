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

export type DbsyncMode = "k8s" | "public" | "rds-proxy";
export type SecretsMode = "pods-by-labels" | "preview-style";

export interface NetworkConfig {
  dbsync?: {
    mode?: DbsyncMode;
  };
  secrets?: {
    mode?: SecretsMode;
  };
  boot?: {
    /** Optional explicit boot pod names to inspect for DB sync creds */
    podNames?: string[];
  };
}

const defaults: Required<NetworkConfig> = {
  dbsync: { mode: "k8s" },
  secrets: { mode: "pods-by-labels" },
  boot: { podNames: [] },
};

export function loadNetworkConfig(namespace: string): Required<NetworkConfig> {
  const configPath = path.resolve(
    __dirname,
    "../networks",
    "well-known",
    namespace,
    "config.json",
  );

  if (!fs.existsSync(configPath)) {
    return defaults;
  }

  try {
    const raw = fs.readFileSync(configPath, "utf-8");
    const parsed = JSON.parse(raw) as NetworkConfig;
    return {
      dbsync: { ...defaults.dbsync, ...parsed.dbsync },
      secrets: { ...defaults.secrets, ...parsed.secrets },
      boot: { ...defaults.boot, ...parsed.boot },
    };
  } catch (error) {
    throw new Error(
      `Failed to parse network config at ${configPath}: ${(error as Error).message}`,
    );
  }
}
