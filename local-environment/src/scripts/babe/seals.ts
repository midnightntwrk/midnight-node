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

// seals <ws> <from> <to> — recover each block's seal signer and compare it with the
// authority the header claims: babe.authorities[idx] at the block (BABE), or
// aura.authorities[slot % n] at the parent (AURA). Exits 1 on any mismatch.
import {
  blake2AsU8a,
  cryptoWaitReady,
  sr25519Verify,
} from "@polkadot/util-crypto";
import { DEFAULT_WS, preDigests, run } from "./common";

const [from, to] = [Number(process.argv[3]), Number(process.argv[4])];

void run(process.argv[2] || DEFAULT_WS, async (api) => {
  await cryptoWaitReady();
  const short = (k: string) => k.slice(2, 10);
  let bad = 0;
  for (let n = from; n <= to; n++) {
    const hash = await api.rpc.chain.getBlockHash(n);
    const header = await api.rpc.chain.getHeader(hash);
    const logs = header.digest.logs;
    const seal = logs[logs.length - 1];
    if (!seal.isSeal) throw new Error(`#${n}: last digest is not a seal`);
    const sig = seal.asSeal[1].toU8a(true);
    const pre = api.createType("Header", {
      ...header.toJSON(),
      digest: { logs: logs.slice(0, -1).map((l) => l.toJSON()) },
    });
    const msg = blake2AsU8a(pre.toU8a(), 256);

    const at = await api.at(hash);
    const parent = await api.at(header.parentHash);
    const engine = (
      (await at.query.consensusEngine.engineState()) as any
    ).toString();
    const babeAt = ((await at.query.babe.authorities()) as any).map((a: any) =>
      a[0].toHex(),
    );
    const babePar = ((await parent.query.babe.authorities()) as any).map(
      (a: any) => a[0].toHex(),
    );
    const auraPar = ((await parent.query.aura.authorities()) as any).map(
      (a: any) => a.toHex(),
    );
    const pool = [...new Set([...babeAt, ...babePar, ...auraPar])];
    const signer = pool.find((k) => sr25519Verify(msg, sig, k));

    const pd = preDigests(header);
    let claimed: string | undefined;
    let what: string;
    if (seal.asSeal[0].toUtf8() === "BABE" && pd.babe) {
      claimed = babeAt[pd.babe.authorityIndex];
      what = `BABE#${pd.babe.authorityIndex} slot ${pd.babe.slot}`;
    } else {
      const slot = pd.auraSlot!;
      claimed = auraPar[Number(slot % BigInt(auraPar.length))];
      what = `aura slot ${slot}`;
    }
    const ok = signer !== undefined && signer === claimed;
    if (!ok) bad++;
    console.log(
      `#${n} ${engine.padEnd(13)} ${what.padEnd(22)} signer=${signer ? short(signer) : "??"} ` +
        `claimed=${claimed ? short(claimed) : "??"} babe.authorities=[${babeAt.map(short).join(",")}] ${ok ? "OK" : "MISMATCH"}`,
    );
  }
  console.log(`RESULT blocks=${to - from + 1} mismatches=${bad}`);
  if (bad) process.exitCode = 1;
});
