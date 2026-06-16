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

import fs from "fs";
import YAML from "yaml";

import { NodeEndpoint } from "./waitForFinality";

const VALIDATOR_ROLE_LABEL = "io.midnight.role";
const VALIDATOR_ROLE = "validator";
const RPC_PORT_LABEL = "io.midnight.rpc-port";

interface PortMapping {
  host: number;
  container: number;
}

/**
 * Walk a docker-compose file and return one host RPC endpoint per service
 * tagged as a validator. A service is a validator if it carries the
 * `io.midnight.role: validator` label; the matching `io.midnight.rpc-port`
 * label tells us which container port hosts substrate RPC, which we then
 * resolve to a host port via the `ports:` mapping.
 *
 * Throws if a labeled service is missing its rpc-port or doesn't publish it,
 * so a misconfiguration is caught at discovery rather than masquerading as
 * "validator silently absent" at probe time.
 */
export function discoverValidatorEndpoints(
  composeFile: string,
): NodeEndpoint[] {
  const raw = fs.readFileSync(composeFile, "utf-8");
  const parsed = YAML.parse(raw) as { services?: Record<string, unknown> };
  const services = parsed?.services;
  if (!services || typeof services !== "object") {
    throw new Error(`Compose file has no services: ${composeFile}`);
  }

  const endpoints: NodeEndpoint[] = [];
  for (const [serviceName, service] of Object.entries(services)) {
    const labels = readLabels(service);
    if (labels.get(VALIDATOR_ROLE_LABEL) !== VALIDATOR_ROLE) continue;

    const containerPortRaw = labels.get(RPC_PORT_LABEL);
    if (!containerPortRaw) {
      throw new Error(
        `Service '${serviceName}' has '${VALIDATOR_ROLE_LABEL}: ${VALIDATOR_ROLE}' but is missing '${RPC_PORT_LABEL}'`,
      );
    }
    const containerPort = Number.parseInt(containerPortRaw, 10);
    if (!Number.isFinite(containerPort)) {
      throw new Error(
        `Service '${serviceName}' has invalid '${RPC_PORT_LABEL}' value: ${containerPortRaw}`,
      );
    }

    const hostPort = findHostPort(service, containerPort);
    if (hostPort === null) {
      throw new Error(
        `Service '${serviceName}' declares '${RPC_PORT_LABEL}: ${containerPort}' but no matching 'ports:' entry publishes it`,
      );
    }

    endpoints.push({
      name: serviceName,
      url: `http://localhost:${hostPort}`,
    });
  }

  if (endpoints.length === 0) {
    throw new Error(
      `No validator services found in compose file ${composeFile}. ` +
        `Expected at least one service with label '${VALIDATOR_ROLE_LABEL}: ${VALIDATOR_ROLE}'.`,
    );
  }

  return endpoints;
}

function readLabels(service: unknown): Map<string, string> {
  const result = new Map<string, string>();
  if (!service || typeof service !== "object") return result;
  const labels = (service as Record<string, unknown>).labels;
  if (!labels) return result;

  if (Array.isArray(labels)) {
    for (const entry of labels) {
      if (typeof entry !== "string") continue;
      const idx = entry.indexOf("=");
      if (idx < 0) {
        result.set(entry, "");
      } else {
        result.set(entry.slice(0, idx), entry.slice(idx + 1));
      }
    }
  } else if (typeof labels === "object") {
    for (const [k, v] of Object.entries(labels as Record<string, unknown>)) {
      result.set(k, String(v));
    }
  }
  return result;
}

function findHostPort(service: unknown, containerPort: number): number | null {
  if (!service || typeof service !== "object") return null;
  const ports = (service as Record<string, unknown>).ports;
  if (!Array.isArray(ports)) return null;

  for (const entry of ports) {
    const mapping = parsePortMapping(entry);
    if (mapping && mapping.container === containerPort) {
      return mapping.host;
    }
  }
  return null;
}

function parsePortMapping(entry: unknown): PortMapping | null {
  if (typeof entry === "string") {
    const [withoutProto] = entry.split("/");
    const parts = withoutProto.split(":");
    if (parts.length < 2) return null;
    const host = Number.parseInt(parts[parts.length - 2], 10);
    const container = Number.parseInt(parts[parts.length - 1], 10);
    if (!Number.isFinite(host) || !Number.isFinite(container)) return null;
    return { host, container };
  }
  if (entry && typeof entry === "object") {
    const obj = entry as Record<string, unknown>;
    const host = Number(obj.published);
    const container = Number(obj.target);
    if (!Number.isFinite(host) || !Number.isFinite(container)) return null;
    return { host, container };
  }
  return null;
}
