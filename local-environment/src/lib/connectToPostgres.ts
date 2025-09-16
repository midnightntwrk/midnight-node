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

import net from "net";
import { execSync, spawn } from "child_process";
import { writeFileSync } from "fs";

const START_PORT = 5432;
const LABEL_SELECTOR = "postgres-operator.crunchydata.com/instance-set=db-01";

function getPostgresPods(ns: string): string[] {
  const cmd = `kubectl get pods -n ${ns} -l ${LABEL_SELECTOR} -o jsonpath='{.items[*].metadata.name}'`;
  const result = execSync(cmd, { encoding: "utf-8" }).trim();
  return result.split(/\s+/).filter(Boolean);
}

function portForwardPod(ns: string, pod: string, localPort: number) {
  const kubectl = spawn(
    "kubectl",
    ["port-forward", `-n`, ns, `pod/${pod}`, `${localPort}:5432`],
    {
      stdio: "inherit",
      detached: true,
    },
  );

  kubectl.unref();
}

function isPortInUse(port: number): Promise<boolean> {
  return new Promise((resolve) => {
    const server = net
      .createServer()
      .once("error", () => resolve(true))
      .once("listening", () => {
        server.close(() => resolve(false));
      })
      .listen(port, "127.0.0.1");
  });
}

export async function connectToPostgres(namespace: string) {
  const podToPort: Record<string, number> = {};
  let port = START_PORT;

  const pods = getPostgresPods(namespace);
  for (const pod of pods) {
    const inUse = await isPortInUse(port);
    if (inUse) {
      console.log(`⚠️  Port ${port} already in use. Skipping ${pod}`);
    } else {
      portForwardPod(namespace, pod, port);
      podToPort[pod] = port;
    }
    port += 1;
  }

  // TODO: Consider removing this method of tracking port mappings between runs
  writeFileSync("port-mapping.json", JSON.stringify(podToPort, null, 2));
  console.log("✅ Port-forwarding started and saved to port-mapping.json");
}