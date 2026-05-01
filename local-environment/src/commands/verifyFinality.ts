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

import { waitForFinality } from "../lib/waitForFinality";

/**
 * Host-mapped RPC ports for every validator in the local-env compose stack.
 * Nodes 4 and 5 run with `STORAGE_SEPARATION=unified`; 1–3 run separate.
 * Probing all of them as a CI gate guards against regressions in either mode:
 * if any validator is stuck or panicking GRANDPA cannot reach 2/3 quorum and
 * finality stalls, so a per-node finality probe surfaces the failure
 * deterministically rather than letting downstream tests time out further along.
 */
const LOCAL_ENV_NODE_RPC_ENDPOINTS = [
  { name: "midnight-node-1", url: "http://localhost:9933" },
  { name: "midnight-node-2", url: "http://localhost:9934" },
  { name: "midnight-node-3", url: "http://localhost:9935" },
  { name: "midnight-node-4 (unified)", url: "http://localhost:9936" },
  { name: "midnight-node-5 (unified)", url: "http://localhost:9944" },
];

export interface VerifyFinalityOptions {
  targetBlock: number;
  timeoutMs: number;
}

export async function verifyFinality(
  network: string,
  options: VerifyFinalityOptions,
): Promise<void> {
  if (network !== "local-env") {
    console.error(
      `verify-finality currently only supports 'local-env', got '${network}'`,
    );
    process.exit(1);
  }

  await waitForFinality(LOCAL_ENV_NODE_RPC_ENDPOINTS, {
    targetBlock: options.targetBlock,
    timeoutMs: options.timeoutMs,
  });
}
