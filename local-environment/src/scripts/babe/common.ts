// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

// Shared helpers for the AURA -> BABE local-env checks (see README.md).

import type { ApiPromise } from "@polkadot/api";
import type { Header } from "@polkadot/types/interfaces";
import { createApi, disconnectApi } from "../../lib/runtimeUpgradeUtils";

export const DEFAULT_WS = "ws://localhost:9933";

/** BABE PreDigest variant tags (sp_consensus_babe::digests::PreDigest). */
export const BABE_KIND: Record<number, string> = {
  1: "Primary",
  2: "SecondaryPlain",
  3: "SecondaryVRF",
};

export interface BabePreDigest {
  kind: number;
  authorityIndex: number;
  slot: bigint;
}

export interface PreDigests {
  /** pre-runtime engine ids in header order */
  order: string[];
  auraSlot?: bigint;
  babe?: BabePreDigest;
}

export function decodeBabe(bytes: Uint8Array): BabePreDigest {
  const b = Buffer.from(bytes);
  return {
    kind: b[0],
    authorityIndex: b.readUInt32LE(1),
    slot: b.readBigUInt64LE(5),
  };
}

export function preDigests(header: Header): PreDigests {
  const out: PreDigests = { order: [] };
  for (const log of header.digest.logs) {
    if (!log.isPreRuntime) continue;
    const [engine, data] = log.asPreRuntime;
    const id = engine.toUtf8();
    out.order.push(id);
    if (id === "aura") out.auraSlot = Buffer.from(data).readBigUInt64LE(0);
    if (id === "BABE") out.babe = decodeBabe(data);
  }
  return out;
}

/** Compact one-line description of a header's digests. */
export function describe(header: Header): string {
  const parts = header.digest.logs.map((log) => {
    if (log.isPreRuntime) {
      const [engine, data] = log.asPreRuntime;
      const id = engine.toUtf8();
      if (id === "aura")
        return `aura(slot ${Buffer.from(data).readBigUInt64LE(0)})`;
      if (id === "BABE") {
        const b = decodeBabe(data);
        return `BABE:${BABE_KIND[b.kind] ?? b.kind}#${b.authorityIndex}(slot ${b.slot})`;
      }
      return id;
    }
    if (log.isSeal) return `seal:${log.asSeal[0].toUtf8()}`;
    if (log.isConsensus) return `cons:${log.asConsensus[0].toUtf8()}`;
    return log.type;
  });
  return `#${header.number.toNumber()} ${parts.join(" ")}`;
}

/** Connect, run, always disconnect; exits non-zero on error. */
export async function run(
  url: string,
  fn: (api: ApiPromise) => Promise<void>,
): Promise<void> {
  const { api, provider } = await createApi(url);
  try {
    await fn(api);
  } catch (e) {
    console.error(e instanceof Error ? e.message : e);
    process.exitCode = 1;
  } finally {
    await disconnectApi(api, provider);
  }
}
