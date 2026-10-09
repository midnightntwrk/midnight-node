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

// M7: watch all imported heads (forks included) and group blocks per slot by BABE kind.
// P+P = primary collision (model: ~3.4 % of slots at c = 1/4); P+Sv = secondary
// orphaned by a same-slot primary (expected under PrimaryAndSecondaryVRFSlots).
// Usage: ts-node forks.ts <ws> [seconds=300]   (use the archive node: it keeps forks)

import { BABE_KIND, preDigests, run } from "./common";

const SHORT: Record<string, string> = {
  Primary: "P",
  SecondaryPlain: "Sp",
  SecondaryVRF: "Sv",
};
const seconds = Number(process.argv[3] ?? 300);

void run(process.argv[2], async (api) => {
  const bySlot = new Map<string, Map<string, string>>();
  const unsub = await api.rpc.chain.subscribeAllHeads((header) => {
    const babe = preDigests(header).babe;
    if (!babe) return;
    const blocks =
      bySlot.get(babe.slot.toString()) ?? new Map<string, string>();
    blocks.set(
      header.hash.toHex(),
      SHORT[BABE_KIND[babe.kind]] ?? String(babe.kind),
    );
    bySlot.set(babe.slot.toString(), blocks);
  });
  await new Promise((resolve) => setTimeout(resolve, seconds * 1000));
  unsub();

  const combos: Record<string, number> = {};
  let blocks = 0;
  for (const kinds of bySlot.values()) {
    blocks += kinds.size;
    const key = [...kinds.values()].sort().join("+");
    combos[key] = (combos[key] ?? 0) + 1;
  }
  const slots = bySlot.size;
  const pct = (n = 0) => `${((100 * n) / slots).toFixed(1)} %`;
  console.log({ seconds, slots, blocks, combos });
  console.log({
    primaryCollisions: pct(combos["P+P"]),
    secondaryOrphanedByPrimary: pct(combos["P+Sv"]),
  });
});
