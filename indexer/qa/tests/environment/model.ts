// This file is part of midnightntwrk/midnight-indexer.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

import fs from 'fs';
import log from '@utils/logging/logger';

export enum EnvironmentName {
  MAINNET = 'mainnet',
  UNDEPLOYED = 'undeployed',
  DEVNET = 'devnet',
  QANET = 'qanet',
  PREVIEW = 'preview',
  PREPROD = 'preprod',
  STAGENET = 'stagenet',
}

export enum CardanoNetwork {
  MAINNET = 'mainnet',
  PREVIEW = 'preview',
  PREPROD = 'preprod',
}

export enum CardanoNetworkType {
  MAINNET = 'mainnet',
  TESTNET = 'testnet',
}

/**
 * Selectable indexer deployment instance.
 *
 * Each deployed environment runs two indexer instances behind the public
 * `indexer.<env>.midnight.network` URL (blue/green). A new indexer version is
 * rolled out to the secondary instance first, so QA can target it explicitly
 * before it is promoted to primary.
 */
export enum IndexerInstance {
  BLUE = 'blue',
  GREEN = 'green',
}

/**
 * Resolves the indexer host for an optional blue/green instance override.
 *
 * - When `rawInstance` is unset/empty, the host is returned unchanged so the
 *   public URL keeps pointing at whichever instance is currently primary.
 * - `blue`/`green` (case-insensitive) rewrite the leading `indexer` label to
 *   `indexer-blue`/`indexer-green` (e.g. `indexer-green.qanet.midnight.network`).
 * - Any other value fails fast, mirroring the `TARGET_ENV` validation style.
 * - The undeployed/localhost environment has no blue/green split, so the
 *   override is ignored there (with a warning) rather than corrupting the host.
 */
export function resolveIndexerHost(
  baseHost: string,
  rawInstance: string | undefined,
  isUndeployed: boolean,
): string {
  const instance = rawInstance?.trim();
  if (!instance) {
    return baseHost;
  }

  if (isUndeployed) {
    log.warn(
      `Ignoring INDEXER_INSTANCE="${instance}": the undeployed environment has no blue/green instances.`,
    );
    return baseHost;
  }

  const normalized = instance.toLowerCase();
  const validInstances = Object.values(IndexerInstance);
  if (!validInstances.includes(normalized as IndexerInstance)) {
    throw new Error(
      `Invalid INDEXER_INSTANCE: "${rawInstance}". ` +
        `Expected one of: ${validInstances.map((name) => `"${name}"`).join(', ')} ` +
        `(or unset to target the primary indexer instance).`,
    );
  }

  return baseHost.replace(/^indexer\./, `indexer-${normalized}.`);
}

type HostConfig = {
  indexerHost: string;
  nodeHost: string;
};

type HostEntry =
  | {
      env: EnvironmentName;
      indexerHost: string;
      nodeHost: string;
    }
  | {
      env: EnvironmentName;
      domain: string;
    };

const hostEntries: HostEntry[] = [
  {
    env: EnvironmentName.UNDEPLOYED,
    indexerHost: 'localhost:8088',
    nodeHost: 'localhost:9944',
  },
  { env: EnvironmentName.QANET, domain: 'qanet.midnight.network' },
  { env: EnvironmentName.DEVNET, domain: 'devnet.midnight.network' },
  { env: EnvironmentName.PREVIEW, domain: 'preview.midnight.network' },
  { env: EnvironmentName.PREPROD, domain: 'preprod.midnight.network' },
  {
    env: EnvironmentName.STAGENET,
    indexerHost: 'indexer.stagenet.shielded.tools',
    nodeHost: 'rpc.stagenet.shielded.tools',
  },
];

const hostConfigByEnvName: Record<EnvironmentName, HostConfig> = hostEntries.reduce(
  (config, entry) => {
    if ('domain' in entry) {
      config[entry.env] = {
        indexerHost: `indexer.${entry.domain}`,
        nodeHost: `rpc.${entry.domain}`,
      };
    } else {
      config[entry.env] = {
        indexerHost: entry.indexerHost,
        nodeHost: entry.nodeHost,
      };
    }
    return config;
  },
  {} as Record<EnvironmentName, HostConfig>,
);

/**
 * Resolves the supported node version for the current environment.
 *
 * This supports both NODE_VERSIONS (new, multi-version format) and
 * NODE_VERSION (legacy, single-version format), as different environments
 * still use different files.
 *
 * NOTE: Once all indexer environments are on >= 3.1.0-rc.1, support for
 * NODE_VERSION can be removed and this helper simplified.
 */
function readSupportedNodeVersion(): string {
  const versionsPath = '../../NODE_VERSIONS';
  const versionPath = '../../NODE_VERSION';

  if (fs.existsSync(versionsPath)) {
    const versions = fs
      .readFileSync(versionsPath, 'utf8')
      .split('\n')
      .map((v) => v.trim())
      .filter(Boolean);

    if (versions.length === 0) {
      throw new Error('NODE_VERSIONS file exists but is empty');
    }

    return versions[1]; // stable choice
  }

  if (fs.existsSync(versionPath)) {
    return fs.readFileSync(versionPath, 'utf8').trim();
  }

  throw new Error('Neither NODE_VERSIONS nor NODE_VERSION file found');
}

export class Environment {
  private readonly envName: EnvironmentName;
  private readonly isUndeployed: boolean;
  private readonly wsProtocol: string;
  private readonly httpProtocol: string;
  private readonly indexerHost: string;
  private readonly networkId: string;
  private readonly nodeHost: string;
  private readonly nodeTag: string;
  private readonly nodeToolkitTag: string;

  constructor() {
    // Setting up environment with error checking
    const rawEnv = process.env.TARGET_ENV;
    const validEnvNames = Object.values(EnvironmentName);

    if (!rawEnv || !validEnvNames.includes(rawEnv as EnvironmentName)) {
      throw new Error(
        `Invalid or missing TARGET_ENV: "${rawEnv}". ` +
          `Expected one of: \n  ${validEnvNames.map((name) => `"${name}"`).join(',\n  ')}`,
      );
    }
    this.envName = rawEnv as EnvironmentName;

    // Setting up the rest of the properties
    this.isUndeployed = this.envName === 'undeployed';
    if (this.isUndeployed) {
      this.wsProtocol = 'ws';
      this.httpProtocol = 'http';
    } else {
      this.wsProtocol = 'wss';
      this.httpProtocol = 'https';
    }

    // These should be now safe to assign as we already
    // checked envName
    this.networkId = this.envName;
    this.indexerHost = resolveIndexerHost(
      hostConfigByEnvName[this.envName].indexerHost,
      process.env.INDEXER_INSTANCE,
      this.isUndeployed,
    );
    this.nodeHost = hostConfigByEnvName[this.envName].nodeHost;
    log.debug(`Using indexer host: ${this.indexerHost}`);

    // What we are actually doing here is the following:
    // 1. If the NODE_TAG is specified as an environment variable, use it. otherwise
    // we read the NODE_VERSION file and use the version from the file.
    // 2. If the NODE_TOOLKIT_VERSION is specified as an environment variable, use it. otherwise
    // we use the same version as the NODE_TAG.
    const supportedNodeVersion = readSupportedNodeVersion();
    this.nodeTag = process.env.NODE_TAG || supportedNodeVersion;
    this.nodeToolkitTag = process.env.NODE_TOOLKIT_TAG || supportedNodeVersion;
    log.debug(`Using NODE_TAG: ${this.nodeTag}`);
    log.debug(`Using NODE_TOOLKIT_TAG: ${this.nodeToolkitTag}`);
  }

  isUndeployedEnv(): boolean {
    return this.isUndeployed;
  }

  getCurrentEnvironmentName(): EnvironmentName {
    return this.envName;
  }

  /**
   * Get the Cardano network connected to a given Midnight environment.
   * @param envName - The Midnight environment name to get the Cardano network for.
   *                  If not provided, the current environment name will be used.
   * @returns The Cardano network.
   */
  getCardanoNetwork(envName: EnvironmentName | undefined = undefined): CardanoNetwork {
    const targetenv: EnvironmentName = envName || this.getCurrentEnvironmentName();
    switch (targetenv) {
      case EnvironmentName.MAINNET:
        return CardanoNetwork.MAINNET;
      case EnvironmentName.PREPROD:
        return CardanoNetwork.PREPROD;
      case EnvironmentName.PREVIEW:
      case EnvironmentName.QANET:
      case EnvironmentName.DEVNET:
      case EnvironmentName.STAGENET:
        return CardanoNetwork.PREVIEW;
      default:
        throw new Error(`Unsupported environment name: ${this.envName}`);
    }
  }

  /**
   * Get the Cardano network type for a given Cardano network.
   * @param network - The Cardano network to get the type for.
   *                  If not provided, the current Cardano network will be used.
   * @returns The Cardano network type.
   */
  getCardanoNetworkType(network: CardanoNetwork | undefined = undefined): CardanoNetworkType {
    const cardanoNetwork = network || this.getCardanoNetwork();
    return cardanoNetwork === 'mainnet' ? CardanoNetworkType.MAINNET : CardanoNetworkType.TESTNET;
  }

  /**
   * Get all the known/supported Midnightenvironment names.
   * @returns All the environment names currently known/supported.
   */
  getAllEnvironmentNames(): string[] {
    return Object.values(EnvironmentName);
  }

  getNetworkId(): string {
    return this.networkId;
  }

  getIndexerHost(): string {
    return this.indexerHost;
  }

  getIndexerHttpBaseURL(): string {
    return `${this.httpProtocol}://${this.indexerHost}`;
  }

  getIndexerWebsocketBaseURL(): string {
    return `${this.wsProtocol}://${this.indexerHost}`;
  }

  getNodeWebsocketBaseURL(): string {
    return `${this.wsProtocol}://${this.nodeHost}`;
  }

  getNodeVersion(): string {
    return this.nodeTag;
  }

  getNodeToolkitVersion(): string {
    return this.nodeToolkitTag;
  }

  /**
   * When INDEXER_INSTANCE targets a blue/green colour, verify the resolved host
   * is actually routed and ready before any tests run.
   *
   * An unrouted colour host does NOT fail at the transport layer: deployed
   * environments use wildcard DNS plus a default ingress backend, so a colour
   * with no ingress rule answers `/ready` with HTTP 404 rather than refusing
   * the connection. We therefore discriminate on the HTTP status:
   *   - 200 → routed and ready, proceed.
   *   - 503 → routed but not caught up yet (instance still syncing).
   *   - 404 / any other status / transport error → no ingress for this colour,
   *     so it is almost certainly the primary (served at the bare host).
   *
   * No-op when INDEXER_INSTANCE is unset or the env has no blue/green split.
   */
  async preflightInstanceSelection(): Promise<void> {
    const instance = process.env.INDEXER_INSTANCE?.trim();
    if (!instance || this.isUndeployed) return;

    const url = `${this.getIndexerHttpBaseURL()}/ready`;
    let status: number;
    try {
      status = (await fetch(url, { signal: AbortSignal.timeout(10_000) })).status;
    } catch (err) {
      throw new Error(
        `INDEXER_INSTANCE="${instance}": could not reach ${this.indexerHost} ` +
          `(${(err as Error).message}). Unset INDEXER_INSTANCE to target the primary instance.`,
        { cause: err },
      );
    }

    if (status === 200) {
      log.info(`INDEXER_INSTANCE="${instance}" → ${this.indexerHost} is routed and ready.`);
      return;
    }
    if (status === 503) {
      throw new Error(
        `INDEXER_INSTANCE="${instance}" (${this.indexerHost}) is routed but NOT caught up yet ` +
          `(HTTP 503). Wait for it to finish syncing before testing against it.`,
      );
    }
    throw new Error(
      `INDEXER_INSTANCE="${instance}" (${this.indexerHost}) returned HTTP ${status} on /ready — ` +
        `no ingress for this colour, so it is almost certainly the PRIMARY (served at the bare ` +
        `${hostConfigByEnvName[this.envName].indexerHost}). Target the other colour, or unset ` +
        `INDEXER_INSTANCE to use the primary.`,
    );
  }
}

export const env = new Environment();
