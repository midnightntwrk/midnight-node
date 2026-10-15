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

// E2 halt catch-up. Stops validators 1-5 for <seconds>, restarts them and logs the rotation
// state per block until <epochs> sidechain epochs after the first block back: the first block
// must rotate once, stamp current + queued with the current epoch and select for epoch + 1;
// then one rotation per epoch. --schedule first closes a scheduleFlip motion at an epoch's
// first slot and halts right after (ScheduledFlip variant). Point it at the archive node
// (it stays up). Grep node logs for UnexpectedEpochChange / ExpectedEpochChange afterwards.
// Usage: ts-node halt.ts <ws> <seconds> [epochs=3] [--schedule]

import { execSync } from "child_process";
import type { ApiPromise } from "@polkadot/api";
import type { Header } from "@polkadot/types/interfaces";
import { run } from "./common";
import { approveScheduleFlip, engineState, log, slotOf } from "./motion";

const VALIDATORS = [1, 2, 3, 4, 5].map((n) => `midnight-node-${n}`).join(" ");

const args = process.argv.slice(2);
const [url, seconds, epochs] = [
  args[0],
  Number(args[1]),
  BigInt(args[2] && !args[2].startsWith("--") ? args[2] : 3),
];
const schedule = args.includes("--schedule");

interface Row {
  slot: bigint;
  epoch: bigint;
  pos: bigint;
  session: number;
  engine: string;
  text: string;
}

async function row(api: ApiPromise, h: Header, spe: bigint): Promise<Row> {
  const slot = slotOf(h);
  const at = await api.at(h.hash);
  const scm = at.query.sessionCommitteeManagement;
  const [session, current, queued, next, babeEpoch] = await Promise.all([
    at.query.session.currentIndex(),
    scm.currentCommittee(),
    scm.queuedCommittee(),
    scm.nextCommittee(),
    at.query.babe.epochIndex(),
  ]);
  const ep = (c: { toJSON(): unknown }) =>
    (c.toJSON() as { epoch?: number } | null)?.epoch ?? "NONE";
  const engine = await engineState(api, h.hash.toHex());
  const [epoch, pos] = [slot / spe, slot % spe];
  return {
    slot,
    epoch,
    pos,
    session: Number(session.toString()),
    engine,
    text:
      `#${h.number.toNumber()} slot=${slot} epoch=${epoch} pos=${pos} session=${session} ` +
      `current=${ep(current)} queued=${ep(queued)} next=${ep(next)} babeEpoch=${babeEpoch} engine=${engine}`,
  };
}

void run(url, async (api) => {
  if (!seconds)
    throw new Error("usage: halt.ts <ws> <seconds> [epochs] [--schedule]");
  const spe = BigInt(api.consts.babe.epochDuration.toString());

  if (schedule) {
    if ((await engineState(api)) !== "Aura")
      throw new Error("--schedule needs engineState = Aura");
    const closeMotion = await approveScheduleFlip(api);
    // close at an epoch's first slot so its last slot falls inside the halt
    await new Promise<void>((resolve, reject) => {
      let sent = false;
      void api.rpc.chain.subscribeNewHeads((h) => {
        if (sent || slotOf(h) % spe !== 0n) return;
        sent = true;
        closeMotion()
          .then(async (hd) => {
            log(`motionClose in ${(await row(api, hd, spe)).text}`);
            resolve();
          })
          .catch(reject);
      });
    });
  }

  const before = await row(api, await api.rpc.chain.getHeader(), spe);
  log(`before: ${before.text}`);
  const since = new Date().toISOString();
  execSync(`docker stop -t 2 ${VALIDATORS}`);
  log(`validators stopped for ${seconds}s (log grep: --since ${since})`);
  await new Promise((r) => setTimeout(r, seconds * 1000));
  execSync(`docker start ${VALIDATORS}`);
  log("validators restarted");

  let first: Row | undefined;
  let prev = before;
  await new Promise<void>((resolve, reject) => {
    void api.rpc.chain
      .subscribeNewHeads(async (h) => {
        const r = await row(api, h, spe);
        if (r.slot <= prev.slot) return;
        const notes: string[] = [];
        if (!first) {
          first = r;
          notes.push(
            `FIRST BACK: ${r.epoch - before.epoch - 1n} epoch(s) skipped, session +${r.session - before.session}`,
          );
        } else if (r.session !== prev.session) {
          notes.push(
            `rotation +${r.session - prev.session}` +
              (r.epoch === prev.epoch ? " — SECOND ROTATION IN ONE EPOCH" : ""),
          );
        }
        if (r.engine !== prev.engine)
          notes.push(`engine ${prev.engine} → ${r.engine}`);
        log(`${r.text}${notes.length ? `  ← ${notes.join("; ")}` : ""}`);
        prev = r;
        if (r.epoch >= first.epoch + epochs && r.pos === spe - 1n) resolve();
      })
      .catch(reject);
  });
});
