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

// M2/M3 dual-digest window: every block carries a BABE SecondaryPlain pre-digest
// placed after the AURA one, with the same slot and authority index = AURA author
// (slot % aura.authorities at the parent).
// Usage: ts-node mirror.ts <ws> <from> <to>

import { run, preDigests } from "./common";

const [url, from, to] = [
  process.argv[2],
  Number(process.argv[3]),
  Number(process.argv[4]),
];

void run(url, async (api) => {
  const bad: string[] = [];
  for (let n = from; n <= to; n++) {
    const header = await api.rpc.chain.getHeader(
      await api.rpc.chain.getBlockHash(n),
    );
    const pre = preDigests(header);
    if (pre.auraSlot === undefined || !pre.babe) {
      bad.push(`#${n} missing digest [${pre.order.join(", ")}]`);
      continue;
    }
    const authorities = (
      await api.query.aura.authorities.at(header.parentHash)
    ).toJSON() as string[];
    const expected = Number(pre.auraSlot % BigInt(authorities.length));
    const fails: string[] = [];
    if (pre.order.indexOf("BABE") < pre.order.indexOf("aura"))
      fails.push("BABE before AURA");
    if (pre.babe.kind !== 2) fails.push(`kind ${pre.babe.kind}`);
    if (pre.babe.slot !== pre.auraSlot)
      fails.push(`slot ${pre.babe.slot} != ${pre.auraSlot}`);
    if (pre.babe.authorityIndex !== expected)
      fails.push(`index ${pre.babe.authorityIndex} != AURA author ${expected}`);
    if (fails.length) bad.push(`#${n} ${fails.join(", ")}`);
  }
  console.log({
    range: `${from}-${to}`,
    ok: to - from + 1 - bad.length,
    bad: bad.length,
  });
  bad.slice(0, 20).forEach((line) => console.log(line));
  if (bad.length) process.exitCode = 1;
});
