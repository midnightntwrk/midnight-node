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

/** Port map e.g. { "psql-dbsync-cardano-0-db-01": 54321 } */
type PortMapping = Record<string, number>;

interface PostgresSecret {
  host: string;
  password: string;
  port: string; // note: env vars are strings; keep as string here
  user: string;
  db: string;
  connectionString?: string;
}

/** Secrets we collect per node */
type NodeRole = "authority" | "boot" | "other";

interface NodeSecrets {
  seed?: string;
  postgres?: PostgresSecret;
  role: NodeRole;
}

type SecretsByNode = Record<string, NodeSecrets>;

const execJsonPath = (cmd: string) =>
  execSync(cmd, { encoding: "utf-8" }).trim().split(/\s+/).filter(Boolean);

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
    const { seed, postgres, role } = nodeSecrets;

    if (seed) {
      env[`${prefix}_SEED`] = seed;
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


export function getSecrets(namespace: string): Record<string, string> {
  const portMapping: Record<string, number> = JSON.parse(
    readFileSync("port-mapping.json", "utf-8"),
  );

  const getPodsByLabel = (label: string): string[] => {
    const cmd = `kubectl get pods -n ${namespace} -l ${label} -o jsonpath='{.items[*].metadata.name}'`;
    const pods = execJsonPath(cmd);
    return pods;
  };

  const execAndParseEnv = (pod: string, fields: string[]): string[] => {
    const echoExpr = fields.map((f) => `$${f}`).join("|");
    const cmd = `kubectl exec -n ${namespace} ${pod} -- sh -c 'echo "${echoExpr}"'`;
    const raw = execSync(cmd, { encoding: "utf-8" }).trim();
    return raw.split("|").map((f) => f.trim());
  };

  const dbSecrets: SecretsByNode = {};

  const processAuthorityPods = () => {
    const pods = getPodsByLabel("midnight.tech/node-type=authority");

    for (const pod of pods) {
      const [seed, host, password, port, user, db] = execAndParseEnv(pod, [
        "SEED_PHRASE",
        "POSTGRES_HOST",
        "POSTGRES_PASSWORD",
        "POSTGRES_PORT",
        "POSTGRES_USER",
        "POSTGRES_DB",
      ]);

      const nodeKey = formatNodeKey(pod);
      const mappedPort = getPortFromMapping(host, portMapping);

      dbSecrets[nodeKey] = {
        seed,
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
    // TODO: fix and write this method
    // writeKeysForAllNodes(dbSecrets, namespace)
  };

  const processBootPods = () => {
    const pods = getPodsByLabel("midnight.tech/node-type=boot");
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
  return envObject;
}
