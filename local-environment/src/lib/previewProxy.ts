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

import net from "net";
import { execSync } from "child_process";
import { startPortForwardWatchdog } from "./portForwardWatchdog";

const BASE_LOCAL_PORT = parseInt(
  process.env.PREVIEW_PROXY_BASE_PORT ?? "15432",
  10,
);

interface ProxySpec {
  proxyName: string;
  envKeys: string[];
  connectionString: string;
  targetHost: string;
  targetPort: number;
}

export async function setupPreviewProxies(
  env: Record<string, string>,
  namespace: string,
): Promise<Record<string, string>> {
  const proxySpecs = buildProxySpecs(env);
  if (proxySpecs.length === 0) {
    console.log("No preview DB targets discovered for proxying");
    return {};
  }

  console.log(
    `Setting up ${proxySpecs.length} preview DB proxy pod(s) and port-forwards`,
  );

  const overrides: Record<string, string> = {};
  let nextPort = BASE_LOCAL_PORT;

  for (const spec of proxySpecs) {
    ensureProxyPod(namespace, spec.proxyName, spec.targetHost, spec.targetPort);

    const localPort = await findAvailablePort(nextPort);
    portForwardProxy(namespace, spec.proxyName, localPort);
    nextPort = localPort + 1;

    const localConnString = rewriteConnectionString(
      spec.connectionString,
      localPort,
    );
    for (const envKey of spec.envKeys) {
      overrides[envKey] = localConnString;
    }
  }

  return overrides;
}

function buildProxySpecs(env: Record<string, string>): ProxySpec[] {
  const proxyById: Record<string, ProxySpec> = {};
  const regex =
    /^DB_SYNC_POSTGRES_CONNECTION_STRING_(?:BOOT_|NODE_)?MIDNIGHT_NODE_(?:BOOT_)?(\d+)_0$/;

  for (const [envKey, connString] of Object.entries(env)) {
    const match = envKey.match(regex);
    if (!match) {
      continue;
    }
    if (!connString) {
      continue;
    }
    const idNum = parseInt(match[1], 10);
    if (Number.isNaN(idNum)) {
      continue;
    }
    const proxyName = `rds-proxy-${idNum}`;
    let targetHost = "";
    let targetPort = 5432;
    try {
      const url = new URL(connString.replace(/^psql:/, "postgres:"));
      targetHost = url.hostname;
      targetPort = url.port ? parseInt(url.port, 10) : 5432;
    } catch (error) {
      console.warn(
        `Failed to parse connection string for ${envKey}: ${(error as Error).message}`,
      );
      continue;
    }

    if (!targetHost) {
      console.warn(`Skipping ${envKey}: missing target host in connection string`);
      continue;
    }

    if (!proxyById[proxyName]) {
      proxyById[proxyName] = {
        proxyName,
        envKeys: [],
        connectionString: connString,
        targetHost,
        targetPort,
      };
    }

    proxyById[proxyName].envKeys.push(envKey);
  }

  return Object.values(proxyById);
}

function ensureProxyPod(
  namespace: string,
  podName: string,
  targetHost: string,
  targetPort: number,
) {
  const manifest = `
apiVersion: v1
kind: Pod
metadata:
  name: ${podName}
  namespace: ${namespace}
  labels:
    app: rds-proxy
spec:
  containers:
    - name: socat
      image: alpine/socat
      command:
        - sh
        - -c
        - |
          echo "Starting proxy to \${POSTGRES_HOST}:\${POSTGRES_PORT}"
          socat TCP-LISTEN:5432,fork,reuseaddr TCP:\${POSTGRES_HOST}:\${POSTGRES_PORT}
      env:
        - name: POSTGRES_HOST
          value: "${targetHost}"
        - name: POSTGRES_PORT
          value: "${targetPort}"
      ports:
        - containerPort: 5432
          name: postgres
          protocol: TCP
      resources:
        requests:
          memory: "64Mi"
          cpu: "50m"
        limits:
          memory: "128Mi"
          cpu: "100m"
  restartPolicy: Always
`;

  try {
    execSync(`kubectl apply -f -`, {
      input: manifest,
      stdio: ["pipe", "inherit", "inherit"],
    });
  } catch (error) {
    console.warn(
      `Failed to apply proxy pod ${podName}: ${(error as Error).message}`,
    );
    return false;
  }
  return true;
}

function portForwardProxy(
  namespace: string,
  podName: string,
  localPort: number,
) {
  if (!waitForPodReady(namespace, podName)) {
    console.warn(
      `Skipping port-forward for ${podName} because it did not become Ready`,
    );
    return;
  }

  startPortForwardWatchdog({
    namespace,
    podName,
    localPort,
    remotePort: 5432,
    label: podName,
  });
}

function rewriteConnectionString(connString: string, localPort: number): string {
  try {
    // URL can parse psql scheme even if non-standard
    const url = new URL(connString);
    if (url.protocol === "psql:") {
      url.protocol = "postgres:";
    }
    // Containers reach the host via host.docker.internal
    url.hostname = "host.docker.internal";
    url.port = `${localPort}`;
    return url.toString();
  } catch (error) {
    console.warn(
      `Failed to rewrite connection string '${connString}': ${(error as Error).message}`,
    );
    return connString;
  }
}

function findAvailablePort(start: number): Promise<number> {
  const MAX_SEARCH = 1000;
  let attempt = start;

  return new Promise((resolve, reject) => {
    const tryNext = () => {
      if (attempt > start + MAX_SEARCH) {
        reject(new Error("No available port found for proxy port-forwarding"));
        return;
      }
      const server = net
        .createServer()
        .once("error", () => {
          attempt += 1;
          tryNext();
        })
        .once("listening", () => {
          server.close(() => resolve(attempt));
        })
        .listen(attempt, "127.0.0.1");
    };
    tryNext();
  });
}

function waitForPodReady(namespace: string, podName: string): boolean {
  try {
    const cmd = `kubectl -n ${namespace} wait --for=condition=Ready pod/${podName} --timeout=30s`;

    execSync(cmd, { stdio: "inherit" });
    return true;
  } catch (error) {
    console.warn(
      `Proxy pod ${podName} not Ready within timeout: ${(error as Error).message}`,
    );
    return false;
  }
}
