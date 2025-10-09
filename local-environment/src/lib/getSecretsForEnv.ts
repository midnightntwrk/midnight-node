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

import { execSync } from "child_process";
import { readFileSync } from "fs";

/** Pod port map e.g. { "psql-dbsync-cardano-0-db-01": 54321 } */
type PortMapping = Record<string, number>;

interface PostgresSecret {
  host: string;
  password: string;
  port: string;
  user: string;
  db: string;
  connectionString?: string;
}

// Roles according to the running networks
type PodNodeRole = "authority" | "boot";

interface NodeSecrets {
  // legacy single seed
  seed?: string;
  auraSeed?: string;
  grandpaSeed?: string;
  crossChainSeed?: string;
  postgres?: PostgresSecret;
  role: PodNodeRole;
}

type SecretsByNode = Record<string, NodeSecrets>;

const execJsonPath = (cmd: string, context: string) => {
  const raw = execSync(cmd, { encoding: "utf-8" }).trim();
  if (!raw) {
    return [];
  }
  const values = raw.split(/\s+/).filter(Boolean);
  return values;
};

const execString = (cmd: string, context: string): string => {
  const raw = execSync(cmd, { encoding: "utf-8" }).trim();
  return raw;
};

const formatNodeKey = (pod: string) => pod.replace(/-/g, "_").toUpperCase();

const getPortFromMapping = (host: string, mapping: PortMapping) => {
  const clusterName = host.replace(/-primary$/, "");
  const entry = Object.entries(mapping).find(([name]) =>
    name.startsWith(clusterName),
  );
  if (!entry) {
    return undefined;
  }
  return entry[1];
};

function convertSecretsToEnvObject(
  secrets: SecretsByNode,
): Record<string, string> {
  const env: Record<string, string> = {};

  for (const [nodeName, nodeSecrets] of Object.entries(secrets)) {
    const prefix = nodeName.toUpperCase();
    const { seed, auraSeed, grandpaSeed, crossChainSeed, postgres, role } =
      nodeSecrets;

    if (seed) {
      env[`${prefix}_SEED`] = seed;
    }

    if (auraSeed) {
      env[`${prefix}_AURA_SEED`] = auraSeed;
    }

    if (grandpaSeed) {
      env[`${prefix}_GRANDPA_SEED`] = grandpaSeed;
    }

    if (crossChainSeed) {
      env[`${prefix}_CROSS_CHAIN_SEED`] = crossChainSeed;
    }

    if (postgres && postgres.connectionString) {
      let roleSegment = "";

      if (role === "boot") {
        roleSegment = "BOOT_";
      } else if (role === "authority") {
        roleSegment = "NODE_";
      }

      const key = `DB_SYNC_POSTGRES_CONNECTION_STRING_${roleSegment}${prefix}`;
      env[key] = postgres.connectionString;
    }
  }

  return env;
}

// TODO: Change this to use AWS SSM
export function getSecrets(namespace: string): Record<string, string> {
  console.log(`loading port mapping from port-mapping.json`);
  const portMappingRaw = readFileSync("port-mapping.json", "utf-8");
  const portMapping: Record<string, number> = JSON.parse(portMappingRaw);
  console.log(
    `loaded ${Object.keys(portMapping).length} port mapping entries`,
  );

  const getPodsByLabel = (label: string): string[] => {
    // TODO: consider kubernetes library instead of manual calls
    const cmd = `kubectl get pods -n ${namespace} -l ${label} -o jsonpath='{.items[*].metadata.name}'`;
    const pods = execJsonPath(cmd, `pods with label ${label}`);
    return pods;
  };

  const execAndParseEnv = (pod: string, fields: string[]): string[] => {
    const echoExpr = fields.map((f) => `$${f}`).join("|");
    const cmd = `kubectl exec -n ${namespace} ${pod} -- sh -c 'echo "${echoExpr}"'`;
    const raw = execSync(cmd, { encoding: "utf-8" }).trim();
    if (!raw) {
      console.warn(
        `pod '${pod}' returned empty env payload for fields [${fields.join(", ")}]
Generated command: ${cmd}`,
      );
      return new Array(fields.length).fill("");
    }
    return raw.split("|").map((f) => f.trim());
  };

  const readSeedFile = (
    pod: string,
    filePath: string | undefined,
    label: string,
  ): string | undefined => {
    const trimmed = filePath?.trim();
    if (!trimmed) {
      return undefined;
    }

    try {
      const cmd = `kubectl exec -n ${namespace} ${pod} -- sh -c 'cat "${trimmed}"'`;
      const seed = execString(cmd, `seed ${label} ${pod}:${trimmed}`);
      if (seed) {
        return seed;
      }
    } catch (error) {
      console.warn(
        `failed to read ${label} seed file '${trimmed}' on pod '${pod}': ${error}`,
      );
    }
    return undefined;
  };

  const dbSecrets: SecretsByNode = {};

  const processAuthorityPods = () => {
    const pods = getPodsByLabel("midnight.tech/node-type=authority");
    console.log(`processing ${pods.length} authority pod(s)`);

    for (const pod of pods) {
      const [
        inlineSeed,
        auraSeedFile,
        grandpaSeedFile,
        crossChainSeedFile,
        host,
        password,
        port,
        user,
        db,
      ] = execAndParseEnv(pod, [
        "SEED_PHRASE",
        "AURA_SEED_FILE",
        "GRANDPA_SEED_FILE",
        "CROSS_CHAIN_SEED_FILE",
        "POSTGRES_HOST",
        "POSTGRES_PASSWORD",
        "POSTGRES_PORT",
        "POSTGRES_USER",
        "POSTGRES_DB",
      ]);

      // Support legacy single seed, if received
      const seed: string | undefined = inlineSeed || undefined;

      // Support any seed files, if received
      const auraSeed = readSeedFile(pod, auraSeedFile, "aura") || undefined;
      const grandpaSeed =
        readSeedFile(pod, grandpaSeedFile, "grandpa") || undefined;
      const crossChainSeed =
        readSeedFile(pod, crossChainSeedFile, "cross-chain") || undefined;

      const nodeKey = formatNodeKey(pod);
      const mappedPort = getPortFromMapping(host, portMapping);

      if (!seed) {
        console.warn(`pod '${pod}' reported empty SEED_PHRASE`);
      }

      dbSecrets[nodeKey] = {
        seed,
        auraSeed,
        grandpaSeed,
        crossChainSeed,
        postgres: {
          host,
          password,
          port,
          user,
          db,
          connectionString: mappedPort
            ? `psql://${user}:${password}@host.docker.internal:${mappedPort}/${db}?ssl-mode=disable`
            : undefined,
        },
        role: "authority",
      };
    }

  };

  const processBootPods = () => {
    const pods = getPodsByLabel("midnight.tech/node-type=boot");
    console.log(`processing ${pods.length} boot pod(s)`);
    for (const pod of pods) {
      const [host, password, port, user, db] = execAndParseEnv(pod, [
        "POSTGRES_HOST",
        "POSTGRES_PASSWORD",
        "POSTGRES_PORT",
        "POSTGRES_USER",
        "POSTGRES_DB",
      ]);

      const nodeKey = formatNodeKey(pod);
      const mappedPort = getPortFromMapping(host, portMapping);

      dbSecrets[nodeKey] = {
        postgres: {
          host,
          password,
          port,
          user,
          db,
          connectionString: mappedPort
            ? `psql://${user}:${password}@host.docker.internal:${mappedPort}/${db}?ssl-mode=disable`
            : undefined,
        },
        role: "boot",
      };
    }
  };

  processAuthorityPods();
  processBootPods();

  const envObject = convertSecretsToEnvObject(dbSecrets);
  console.log(
    `prepared env keys: ${Object.keys(envObject).sort().join(", ")}`,
  );
  return envObject;
}
