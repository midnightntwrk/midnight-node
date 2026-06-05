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

// Per-runner port isolation for the local-env stack.
//
// The self-hosted CI host runs up to 8 GitHub Actions runner slots against a
// single shared Docker daemon. The local-env stack publishes ~22 fixed host
// ports and uses fixed container names, so two jobs on the same host collide
// ("port is already allocated" / name conflict). To let jobs run concurrently
// we give each runner slot a disjoint block of host ports plus a unique compose
// project name and container-name suffix.
//
// Slot 0 (the default, when LOCALENV_RUNNER_SLOT is unset or not a positive
// integer) preserves the historical behaviour exactly: legacy host ports,
// project name "local-env", and no container-name suffix. This keeps local
// developer workflows and existing docs/tooling unchanged.
//
// NOTE: the equivalent computation is mirrored in pure bash in
// `.github/actions/local-environment-tests/action.yml` (the host has no
// guaranteed node deps when it needs the values for e2e build-args). The
// ordered PORT_SPEC list and the `BASE_PORT`/`BLOCK` constants below MUST stay
// in lockstep with that script.

/** First host port of the per-runner range. Chosen to sit clear of the legacy
 * defaults and below the typical ephemeral range (32768+). */
export const BASE_PORT = 21000;

/** Host ports reserved per runner slot. Must exceed PORT_SPEC.length so that
 * adjacent slots' blocks never overlap. */
export const BLOCK = 64;

/**
 * Ordered list of every published host port. The index in this array is the
 * port's offset within a runner slot's block, so entries MUST NOT be reordered
 * (append only). `envVar` is the compose interpolation variable; `legacy` is
 * the historical host port used at slot 0.
 */
export const PORT_SPEC: { envVar: string; legacy: number }[] = [
  { envVar: "CARDANO_NODE_HOST_PORT", legacy: 32000 },
  { envVar: "CARDANO_SOCAT_HOST_PORT", legacy: 30000 },
  { envVar: "KUPO_HOST_PORT", legacy: 1442 },
  { envVar: "OGMIOS_HOST_PORT", legacy: 1337 },
  { envVar: "POSTGRES_HOST_PORT", legacy: 5432 },
  { envVar: "INDEXER_API_HOST_PORT", legacy: 8088 },
  { envVar: "NATS_HOST_PORT", legacy: 4222 },
  { envVar: "MN1_P2P_HOST_PORT", legacy: 30333 },
  { envVar: "MN1_RPC_HOST_PORT", legacy: 9933 },
  { envVar: "MN1_PROM_HOST_PORT", legacy: 9615 },
  { envVar: "MN2_P2P_HOST_PORT", legacy: 30334 },
  { envVar: "MN2_RPC_HOST_PORT", legacy: 9934 },
  { envVar: "MN2_PROM_HOST_PORT", legacy: 9616 },
  { envVar: "MN3_P2P_HOST_PORT", legacy: 30335 },
  { envVar: "MN3_RPC_HOST_PORT", legacy: 9935 },
  { envVar: "MN3_PROM_HOST_PORT", legacy: 9617 },
  { envVar: "MN4_P2P_HOST_PORT", legacy: 30336 },
  { envVar: "MN4_RPC_HOST_PORT", legacy: 9936 },
  { envVar: "MN4_PROM_HOST_PORT", legacy: 9618 },
  { envVar: "MN5_P2P_HOST_PORT", legacy: 30337 },
  { envVar: "MN5_RPC_HOST_PORT", legacy: 9944 },
  { envVar: "MN5_PROM_HOST_PORT", legacy: 9619 },
];

export interface LocalEnvLayout {
  /** 0 = default/legacy single-tenant; 1..N = per-runner slot. */
  slot: number;
  /** Compose project name (isolates networks and volumes). */
  projectName: string;
  /** Suffix appended to every `container_name` (isolates host-global names). */
  nameSuffix: string;
  /** Compose interpolation var -> resolved host port. */
  hostPorts: Record<string, number>;
}

/**
 * Read the runner slot from the environment. Anything that is not a positive
 * integer (unset, empty, "0", non-numeric) resolves to slot 0 — the legacy,
 * single-tenant layout.
 */
export function resolveSlot(env: NodeJS.ProcessEnv = process.env): number {
  const raw = env.LOCALENV_RUNNER_SLOT;
  if (!raw) return 0;
  const n = Number.parseInt(raw, 10);
  if (!Number.isInteger(n) || n < 1) return 0;
  return n;
}

/** Compute the full layout (project name, suffix, host ports) for a slot. */
export function computeLayout(slot: number): LocalEnvLayout {
  if (slot < 1) {
    const hostPorts: Record<string, number> = {};
    for (const spec of PORT_SPEC) hostPorts[spec.envVar] = spec.legacy;
    return {
      slot: 0,
      projectName: "local-env",
      nameSuffix: "",
      hostPorts,
    };
  }

  const blockStart = BASE_PORT + (slot - 1) * BLOCK;
  const lastPort = blockStart + PORT_SPEC.length - 1;
  if (PORT_SPEC.length > BLOCK) {
    throw new Error(
      `PORT_SPEC has ${PORT_SPEC.length} ports but BLOCK is ${BLOCK}; adjacent runner slots would overlap`,
    );
  }
  if (lastPort > 65535) {
    throw new Error(
      `Runner slot ${slot} maps to host port ${lastPort}, which exceeds 65535. Reduce slot count or BASE_PORT.`,
    );
  }

  const hostPorts: Record<string, number> = {};
  PORT_SPEC.forEach((spec, index) => {
    hostPorts[spec.envVar] = blockStart + index;
  });

  return {
    slot,
    projectName: `local-env-r${slot}`,
    nameSuffix: `-r${slot}`,
    hostPorts,
  };
}

/**
 * Build the environment overrides docker compose needs to publish this slot's
 * ports and namespace its project/containers. Merge the result over the base
 * env handed to `docker compose`.
 */
export function layoutEnv(layout: LocalEnvLayout): Record<string, string> {
  const env: Record<string, string> = {
    COMPOSE_PROJECT_NAME: layout.projectName,
    LOCALENV_NAME_SUFFIX: layout.nameSuffix,
    LOCALENV_RUNNER_SLOT: String(layout.slot),
  };
  for (const [key, port] of Object.entries(layout.hostPorts)) {
    env[key] = String(port);
  }
  return env;
}

/** Convenience: layout for the current process environment. */
export function currentLayout(
  env: NodeJS.ProcessEnv = process.env,
): LocalEnvLayout {
  return computeLayout(resolveSlot(env));
}
