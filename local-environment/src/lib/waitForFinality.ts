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

export interface NodeEndpoint {
  name: string;
  url: string;
}

export interface WaitForFinalityOptions {
  targetBlock?: number;
  timeoutMs?: number;
  pollIntervalMs?: number;
}

/**
 * Wait until every supplied node has finalized at least `targetBlock`. Useful
 * as a post-startup health gate: if any validator is stuck or panicking
 * GRANDPA cannot reach 2/3 quorum and finality stalls, so a per-node finality
 * probe surfaces the regression deterministically rather than letting
 * downstream tests time out further along.
 */
export async function waitForFinality(
  nodes: NodeEndpoint[],
  {
    targetBlock = 1,
    timeoutMs = 5 * 60_000,
    pollIntervalMs = 2_000,
  }: WaitForFinalityOptions = {},
): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  const remaining = new Set(nodes.map((n) => n.name));
  const byName = new Map(nodes.map((n) => [n.name, n]));

  console.log(
    `⏳ Waiting for finalized block >= ${targetBlock} on: ${nodes
      .map((n) => n.name)
      .join(", ")}`,
  );

  while (remaining.size > 0) {
    const probes = Array.from(remaining).map(async (name) => {
      const node = byName.get(name);
      if (!node) return;
      const finalized = await getFinalizedNumber(node.url).catch(() => null);
      if (finalized !== null && finalized >= targetBlock) {
        console.log(`✅ ${name}: finalized #${finalized}`);
        remaining.delete(name);
      }
    });
    await Promise.all(probes);

    if (remaining.size === 0) break;

    if (Date.now() >= deadline) {
      const stuck = Array.from(remaining).join(", ");
      throw new Error(
        `Timed out after ${timeoutMs}ms waiting for finalized block >= ${targetBlock} on: ${stuck}`,
      );
    }

    await sleep(pollIntervalMs);
  }
}

async function getFinalizedNumber(url: string): Promise<number | null> {
  const finalizedHash = await rpc<string>(url, "chain_getFinalizedHead", []);
  if (!finalizedHash) return null;
  const header = await rpc<{ number: string } | null>(url, "chain_getHeader", [
    finalizedHash,
  ]);
  if (!header || typeof header.number !== "string") return null;
  return parseInt(header.number, 16);
}

async function rpc<T>(
  url: string,
  method: string,
  params: unknown[],
): Promise<T | null> {
  const response = await fetch(url, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ jsonrpc: "2.0", id: 1, method, params }),
  });
  if (!response.ok) return null;
  const body = (await response.json()) as { result?: T };
  return body.result ?? null;
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
