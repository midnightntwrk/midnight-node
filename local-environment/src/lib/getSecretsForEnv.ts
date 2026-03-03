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

import { execSync } from "child_process";
import { readFileSync } from "fs";
import { loadNetworkConfig } from "./networkConfig";

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
  seed?: string;
  auraSeed?: string;
  grandpaSeed?: string;
  crossChainSeed?: string;
  postgres?: PostgresSecret;
  role: PodNodeRole;
  envPrefix?: string;
}

type SecretsByNode = Record<string, NodeSecrets>;

const AUTHORITY_ENV_FIELDS = [
  "SEED_PHRASE",
  "AURA_SEED_FILE",
  "GRANDPA_SEED_FILE",
  "CROSS_CHAIN_SEED_FILE",
  "POSTGRES_HOST",
  "POSTGRES_PASSWORD",
  "POSTGRES_PORT",
  "POSTGRES_USER",
  "POSTGRES_DB",
] as const;

const BOOT_ENV_FIELDS = [
  "POSTGRES_HOST",
  "POSTGRES_PASSWORD",
  "POSTGRES_PORT",
  "POSTGRES_USER",
  "POSTGRES_DB",
] as const;

const SEED_ENV_KEYS = [
  ["seed", "SEED"],
  ["auraSeed", "AURA_SEED"],
  ["grandpaSeed", "GRANDPA_SEED"],
  ["crossChainSeed", "CROSS_CHAIN_SEED"],
] as const;

// TODO: Change this to use AWS SSM
export function getSecrets(namespace: string): Record<string, string> {
  const networkConfig = loadNetworkConfig(namespace);

  if (networkConfig.secrets.mode === "preview-style") {
    return getPreviewSecrets(namespace);
  }

  const portMapping = loadPortMapping();

  const secrets: SecretsByNode = {};
  collectAuthorityPods(namespace, portMapping, secrets);
  collectBootPods(namespace, portMapping, secrets, networkConfig.boot.podNames);

  const envObject = convertSecretsToEnvObject(secrets);
  return envObject;
}

function loadPortMapping(): PortMapping {
  console.log("loading port mapping from port-mapping.json");
  try {
    const portMappingRaw = readFileSync("port-mapping.json", "utf-8");
    const portMapping = JSON.parse(portMappingRaw) as PortMapping;
    console.log(
      `loaded ${Object.keys(portMapping).length} port mapping entries`,
    );
    return portMapping;
  } catch (error) {
    throw new Error(
      `failed to read port-mapping.json: ${(error as Error).message}`,
    );
  }
}

function collectAuthorityPods(
  namespace: string,
  portMapping: PortMapping,
  secrets: SecretsByNode,
) {
  const pods = listPods(namespace, "midnight.tech/node-type=authority");
  console.log(`processing ${pods.length} authority pod(s)`);

  for (const pod of pods) {
    const envValues = readPodEnv(namespace, pod, AUTHORITY_ENV_FIELDS);
    const nodeKey = formatNodeKey(pod);

    const seed = envValues.SEED_PHRASE?.trim() || undefined;

    const auraSeed = readSeedFile(
      namespace,
      pod,
      envValues.AURA_SEED_FILE,
      "aura",
    );
    const grandpaSeed = readSeedFile(
      namespace,
      pod,
      envValues.GRANDPA_SEED_FILE,
      "grandpa",
    );
    const crossChainSeed = readSeedFile(
      namespace,
      pod,
      envValues.CROSS_CHAIN_SEED_FILE,
      "cross-chain",
    );

    secrets[nodeKey] = {
      seed,
      auraSeed,
      grandpaSeed,
      crossChainSeed,
      postgres: buildPostgresSecret(envValues, portMapping),
      role: "authority",
    };
  }
}

function collectBootPods(
  namespace: string,
  portMapping: PortMapping,
  secrets: SecretsByNode,
  explicitPods: string[] = [],
) {
  const pods =
    explicitPods.length > 0
      ? explicitPods
      : listPods(namespace, "midnight.tech/node-type=boot");
  console.log(`processing ${pods.length} boot pod(s)`);

  for (const pod of pods) {
    const envValues = readPodEnv(namespace, pod, BOOT_ENV_FIELDS);
    const nodeKey = formatNodeKey(pod);

    secrets[nodeKey] = {
      postgres: buildPostgresSecret(envValues, portMapping),
      role: "boot",
    };
  }
}

function buildPostgresSecret(
  envValues: Record<string, string>,
  portMapping: PortMapping,
): PostgresSecret | undefined {
  const host = envValues.POSTGRES_HOST?.trim() ?? "";
  const password = envValues.POSTGRES_PASSWORD?.trim() ?? "";
  const port = envValues.POSTGRES_PORT?.trim() ?? "";
  const user = envValues.POSTGRES_USER?.trim() ?? "";
  const db = envValues.POSTGRES_DB?.trim() ?? "";

  if (!(host || password || port || user || db)) {
    return undefined;
  }

  const secret: PostgresSecret = {
    host,
    password,
    port,
    user,
    db,
  };

  const mappedPort = host ? getPortFromMapping(host, portMapping) : undefined;
  if (mappedPort) {
    secret.connectionString = `psql://${user}:${password}@host.docker.internal:${mappedPort}/${db}?sslmode=disable`;
  }
  return secret;
}

function readPodEnv(
  namespace: string,
  pod: string,
  fields: readonly string[],
): Record<string, string> {
  if (fields.length === 0) {
    return {};
  }

  const echoExpr = fields.map((field) => `$${field}`).join("|");
  const cmd = `kubectl exec -n ${namespace} ${pod} -- sh -c 'echo "${echoExpr}"'`;

  try {
    const raw = execSync(cmd, { encoding: "utf-8" }).trim();
    const pieces = raw ? raw.split("|") : [];

    return Object.fromEntries(
      fields.map((field, index) => [field, (pieces[index] ?? "").trim()]),
    );
  } catch (error) {
    console.warn(
      `pod '${pod}' failed to read env fields [${fields.join(", ")}]: ${(error as Error).message}`,
    );
    return Object.fromEntries(fields.map((field) => [field, ""]));
  }
}

function readSeedFile(
  namespace: string,
  pod: string,
  filePath: string | undefined,
  label: string,
): string | undefined {
  const trimmed = filePath?.trim();
  if (!trimmed) {
    return undefined;
  }

  try {
    const cmd = `kubectl exec -n ${namespace} ${pod} -- sh -c 'cat "${trimmed}"'`;
    const seed = execSync(cmd, { encoding: "utf-8" }).trim();
    return seed || undefined;
  } catch (error) {
    console.warn(
      `failed to read ${label} seed file '${trimmed}' on pod '${pod}': ${(error as Error).message}`,
    );
    return undefined;
  }
}

function listPods(namespace: string, label: string): string[] {
  const cmd = `kubectl get pods -n ${namespace} -l ${label} -o jsonpath='{.items[*].metadata.name}'`;
  try {
    const raw = execSync(cmd, { encoding: "utf-8" }).trim();
    if (!raw) {
      return [];
    }
    return raw.split(/\s+/).filter(Boolean);
  } catch (error) {
    console.warn(
      `failed to list pods for label '${label}': ${(error as Error).message}`,
    );
    return [];
  }
}

function convertSecretsToEnvObject(
  secrets: SecretsByNode,
): Record<string, string> {
  const env: Record<string, string> = {};

  for (const [nodeName, nodeSecrets] of Object.entries(secrets)) {
    const prefix = (nodeSecrets.envPrefix ?? nodeName).toUpperCase();

    for (const [property, suffix] of SEED_ENV_KEYS) {
      const value = nodeSecrets[property];
      if (typeof value === "string" && value) {
        env[`${prefix}_${suffix}`] = value;
      }
    }

    const connectionString = nodeSecrets.postgres?.connectionString;
    if (connectionString) {
      const roleSegment = nodeSecrets.role === "boot" ? "BOOT_" : "NODE_";
      const key = `DB_SYNC_POSTGRES_CONNECTION_STRING_${roleSegment}${prefix}`;
      env[key] = connectionString;
    }
  }

  return env;
}

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

const PREVIEW_ENV_FIELDS = [
  "DB_SYNC_POSTGRES_CONNECTION_STRING",
  "SEED_PHRASE",
  "AURA_SEED_FILE",
  "GRANDPA_SEED_FILE",
  "CROSS_CHAIN_SEED_FILE",
] as const;

function getPreviewSecrets(namespace: string): Record<string, string> {
  const pods = listPreviewValidatorPods(namespace);
  console.log(`processing ${pods.length} preview validator pod(s)`);

  const secrets: SecretsByNode = {};

  for (const pod of pods) {
    const envValues = readPodEnv(namespace, pod, PREVIEW_ENV_FIELDS);
    const validatorId = parseValidatorId(pod);
    if (!validatorId) {
      console.warn(
        `skipping pod '${pod}' because validator id could not be parsed`,
      );
      continue;
    }

    const auraSeed = readSeedFile(
      namespace,
      pod,
      envValues.AURA_SEED_FILE,
      "aura",
    );
    const grandpaSeed = readSeedFile(
      namespace,
      pod,
      envValues.GRANDPA_SEED_FILE,
      "grandpa",
    );
    const crossChainSeed = readSeedFile(
      namespace,
      pod,
      envValues.CROSS_CHAIN_SEED_FILE,
      "cross-chain",
    );
    const seed =
      envValues.SEED_PHRASE?.trim() ||
      auraSeed ||
      grandpaSeed ||
      crossChainSeed;

    const connectionString =
      envValues.DB_SYNC_POSTGRES_CONNECTION_STRING?.trim() ?? "";

    secrets[pod] = {
      seed,
      auraSeed,
      grandpaSeed,
      crossChainSeed,
      role: "authority",
      envPrefix: `MIDNIGHT_NODE_${validatorId}_0`,
      postgres: connectionString
        ? {
            host: "",
            password: "",
            port: "",
            user: "",
            db: "",
            connectionString,
          }
        : undefined,
    };
  }

  const bootPods = listPreviewBootPods(namespace);
  console.log(`processing ${bootPods.length} preview boot pod(s)`);
  for (const pod of bootPods) {
    const envValues = readPodEnv(
      namespace,
      pod,
      ["DB_SYNC_POSTGRES_CONNECTION_STRING"] as const,
    );
    const bootId = parseBootId(pod);
    if (!bootId) {
      console.warn(`skipping boot pod '${pod}' because id could not be parsed`);
      continue;
    }

    const connectionString =
      envValues.DB_SYNC_POSTGRES_CONNECTION_STRING?.trim() ?? "";

    secrets[pod] = {
      role: "boot",
      envPrefix: `MIDNIGHT_NODE_BOOT_${bootId}_0`,
      postgres: connectionString
        ? {
            host: "",
            password: "",
            port: "",
            user: "",
            db: "",
            connectionString,
          }
        : undefined,
    };
  }

  return convertSecretsToEnvObject(secrets);
}

function listPreviewValidatorPods(namespace: string): string[] {
  const cmd = `kubectl get pods -n ${namespace} -o jsonpath='{.items[*].metadata.name}'`;
  try {
    const raw = execSync(cmd, { encoding: "utf-8" }).trim();
    if (!raw) {
      return [];
    }
    return raw
      .split(/\s+/)
      .filter((name) => /midnight-node-validator/i.test(name));
  } catch (error) {
    console.warn(
      `failed to list preview validator pods: ${(error as Error).message}`,
    );
    return [];
  }
}

function parseValidatorId(pod: string): string | undefined {
  const match = pod.match(/validator-(\d+)-0/);
  if (!match) {
    return undefined;
  }
  return match[1].padStart(2, "0");
}

function listPreviewBootPods(namespace: string): string[] {
  const cmd = `kubectl get pods -n ${namespace} -o jsonpath='{.items[*].metadata.name}'`;
  try {
    const raw = execSync(cmd, { encoding: "utf-8" }).trim();
    if (!raw) {
      return [];
    }
    return raw
      .split(/\s+/)
      .filter((name) => /midnight-node-boot/i.test(name));
  } catch (error) {
    console.warn(
      `failed to list preview boot pods: ${(error as Error).message}`,
    );
    return [];
  }
}

function parseBootId(pod: string): string | undefined {
  const match = pod.match(/boot-(\d+)-0/);
  if (!match) {
    return undefined;
  }
  return match[1].padStart(2, "0");
}
