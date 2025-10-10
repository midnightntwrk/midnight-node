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

import fs from "fs";
import path from "path";
import { cryptoWaitReady } from "@polkadot/util-crypto";
import { Keyring } from "@polkadot/keyring";

// "Legacy" describes the old ways of representing seeds on pods. Due to the difference
// in how to represent them, it's easiest to include it here to distinguish
type SeedCategory = "aura" | "grandpa" | "crossChain" | "legacy";
type PrimarySeedCategory = Exclude<SeedCategory, "legacy">;

const KEY_TYPES = [
  {
    id: "aura" as const,
    scheme: "sr25519" as const,
    seedCategory: "aura" as const,
  },
  {
    id: "gran" as const,
    scheme: "ed25519" as const,
    seedCategory: "grandpa" as const,
  },
  {
    id: "crch" as const,
    scheme: "ecdsa" as const,
    seedCategory: "crossChain" as const,
  },
  // TODO: Support BEEFY key files in pods.
  // { id: "beef" as const, scheme: "ecdsa" as const, seedCategory: "beefy" as const },
];

// Like Substrate KeyTypeId
type KeyTypeId = (typeof KEY_TYPES)[number]["id"];
type KeyScheme = (typeof KEY_TYPES)[number]["scheme"];

// Exceptions for networks such as node-dev-01, which are running with qanet keys
const CHAIN_ID_OVERRIDE: Record<string, string> = {
  "node-dev-01": "qanet",
};

interface NamespaceKeystoreOptions {
  namespace: string;
  env: Record<string, string>;
  // Override base path for testing
  basePath?: string;
}

interface NodeSeedSet {
  nodeDir: string;
  index: number;
  seeds: Partial<Record<SeedCategory, string>>;
}

const KEY_PREFIX_HEX = Object.fromEntries(
  KEY_TYPES.map(({ id }) => [id, Buffer.from(id).toString("hex")]),
) as Record<KeyTypeId, string>;

// TODO: BEEFY
const POD_KEY_FILE_PATTERN =
  /^MIDNIGHT_NODE_(\d+)(?:_[0-9]+)?_(AURA|GRANDPA|CROSS_CHAIN)_SEED$/;
const POD_LEGACY_KEY_PATTERN = /^MIDNIGHT_NODE_(\d+)(?:_[0-9]+)?_SEED$/;
const CATEGORY_MAP: Record<string, PrimarySeedCategory> = {
  AURA: "aura",
  GRANDPA: "grandpa",
  CROSS_CHAIN: "crossChain",
  // BEEFY: "beefy",
};

export async function prepareNamespaceKeystore(
  options: NamespaceKeystoreOptions,
): Promise<void> {
  const { namespace, env, basePath } = options;

  const networkBasePath = resolveNetworkBasePath(namespace, basePath);
  if (!fs.existsSync(networkBasePath)) {
    throw new Error(
      `Network directory missing for namespace '${namespace}' at ${networkBasePath}`,
    );
  }
  console.log(`using network base path ${networkBasePath}`);

  const dataPath = path.join(networkBasePath, "data");
  mkDirectory(dataPath);

  const chainId = resolveChainId(namespace);
  console.log(`resolved chain id '${chainId}'`);

  const seeds = extractNodeSeeds(env);
  if (seeds.length === 0) {
    console.warn(
      `No node seeds found in environment for namespace '${namespace}'. Skipping keystore preparation.`,
    );
    return;
  }

  await cryptoWaitReady();

  for (const seedSet of seeds) {
    writeNodeKeystore({ namespace, chainId, dataPath, seedSet });
  }

  console.log(
    `Prepared keystore files for ${seeds.length} node(s) in namespace '${namespace}'.`,
  );
}

function resolveNetworkBasePath(namespace: string, basePath?: string): string {
  if (basePath) {
    return path.resolve(basePath);
  }

  return path.resolve(__dirname, "../networks", "well-known", namespace);
}

function resolveChainId(namespace: string): string {
  const override = CHAIN_ID_OVERRIDE[namespace];
  if (override) {
    console.log(
      `using chain id override '${override}' for namespace '${namespace}'`,
    );
    return override;
  }

  const candidates = [
    path.resolve(__dirname, "../../../res", namespace, "chain-spec-raw.json"),
    path.resolve(__dirname, "../../../res", namespace, "chain-spec.json"),
  ];

  for (const candidate of candidates) {
    if (!fs.existsSync(candidate)) {
      continue;
    }

    try {
      const raw = JSON.parse(fs.readFileSync(candidate, "utf-8"));
      if (typeof raw?.id === "string" && raw.id.length > 0) {
        return raw.id;
      }
    } catch (error) {
      throw new Error(`Failed to parse chain spec at ${candidate}: ${error}`);
    }
  }

  throw new Error(
    `Unable to determine chain id for namespace '${namespace}'. Checked chain-spec-raw.json and chain-spec.json`,
  );
}

function mkDirectory(pathname: string) {
  fs.mkdirSync(pathname, { recursive: true });
}

function extractNodeSeeds(env: Record<string, string>): NodeSeedSet[] {
  const nodes = new Map<number, NodeSeedSet>();

  for (const [key, rawValue] of Object.entries(env)) {
    if (!key.startsWith("MIDNIGHT_NODE_") || !key.endsWith("_SEED")) {
      continue;
    }

    const match = parseSeedEnvKey(key);
    if (!match) {
      continue;
    }

    const trimmed = rawValue?.trim();
    if (!trimmed) {
      console.warn(`env var '${key}' is empty; skipping keystore entry`);
      continue;
    }

    const node = ensureNodeEntry(nodes, match.index);
    node.seeds[match.category] = trimmed;
    console.log(
      `captured ${match.category} seed for ${node.nodeDir} from env '${key}'`,
    );
  }

  const seeds = Array.from(nodes.values()).sort((a, b) => a.index - b.index);

  if (seeds.length === 0) {
    console.warn(`no seed environment variables matched expected patterns`);
  } else {
    console.log(
      `assembled seed entries for nodes: ${seeds
        .map(({ nodeDir }) => nodeDir)
        .join(", ")}`,
    );
  }

  return seeds;
}

function parseSeedEnvKey(
  key: string,
): { index: number; category: SeedCategory } | undefined {
  const categoryMatch = key.match(POD_KEY_FILE_PATTERN);
  if (categoryMatch) {
    const index = parseNodeIndex(categoryMatch[1], key);
    if (index === undefined) {
      return undefined;
    }

    const mapped = CATEGORY_MAP[categoryMatch[2]];
    if (!mapped) {
      return undefined;
    }

    return { index, category: mapped };
  }

  const legacyMatch = key.match(POD_LEGACY_KEY_PATTERN);
  if (!legacyMatch) {
    return undefined;
  }

  const index = parseNodeIndex(legacyMatch[1], key);
  if (index === undefined) {
    return undefined;
  }

  return { index, category: "legacy" };
}

function parseNodeIndex(raw: string, envKey: string): number | undefined {
  const index = Number.parseInt(raw, 10);
  if (Number.isNaN(index)) {
    console.warn(
      `env var '${envKey}' produced NaN index; skipping keystore entry`,
    );
    return undefined;
  }
  return index;
}

function ensureNodeEntry(
  nodes: Map<number, NodeSeedSet>,
  index: number,
): NodeSeedSet {
  let entry = nodes.get(index);
  if (!entry) {
    entry = { nodeDir: `node-${index}`, index, seeds: {} };
    nodes.set(index, entry);
  }
  return entry;
}

function writeNodeKeystore(options: {
  namespace: string;
  chainId: string;
  dataPath: string;
  seedSet: NodeSeedSet;
}) {
  const { namespace, chainId, dataPath, seedSet } = options;
  const nodeDataPath = path.join(dataPath, seedSet.nodeDir);
  const keystorePath = path.join(nodeDataPath, "chains", chainId, "keystore");

  mkDirectory(keystorePath);

  console.log(
    `populating keystore for ${namespace}/${seedSet.nodeDir} at ${keystorePath}`,
  );

  for (const { id, scheme, seedCategory } of KEY_TYPES) {
    const { value: seedValue, source } = resolveSeedValue(
      seedSet.seeds,
      seedCategory,
    );

    if (!seedValue) {
      console.warn(
        `skipping ${id} key for ${namespace}/${seedSet.nodeDir} – missing seed (category ${seedCategory})`,
      );
      continue;
    }

    const publicKey = derivePublicKey(seedValue, scheme);
    const filename = keystoreFileName(id, publicKey);
    const filepath = path.join(keystorePath, filename);
    fs.writeFileSync(filepath, JSON.stringify(seedValue));
    console.log(
      `wrote ${id} key '${filename}' for ${namespace}/${seedSet.nodeDir} using ${source} seed`,
    );
  }
}

function resolveSeedValue(
  seeds: NodeSeedSet["seeds"],
  category: PrimarySeedCategory,
): { value?: string; source?: SeedCategory } {
  const direct = seeds[category];
  if (direct) {
    return { value: direct, source: category };
  }

  if (seeds.legacy) {
    return { value: seeds.legacy, source: "legacy" };
  }

  return {};
}

function keystoreFileName(type: KeyTypeId, publicKey: Uint8Array): string {
  const prefix = KEY_PREFIX_HEX[type];
  return `${prefix}${Buffer.from(publicKey).toString("hex")}`;
}

function derivePublicKey(seed: string, scheme: KeyScheme): Uint8Array {
  const trimmed = seed.trim();
  const keyring = new Keyring({ type: scheme });
  const pair = keyring.addFromUri(trimmed);
  return pair.publicKey;
}
