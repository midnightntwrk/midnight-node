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

// M8 flip postponement. Schedules the flip through the federated authority and closes
// the motion so it lands in an epoch's last block (c), then stops the author of the next
// epoch's last slot and restarts it once that slot has passed (a). Logs every block until
// the flip. Needs engineState = Aura and non-empty babe.authorities.
// Usage: ts-node postpone.ts [ws]

import { exec } from "child_process";
import type { ApiPromise } from "@polkadot/api";
import type { SubmittableExtrinsic } from "@polkadot/api/promise/types";
import type { KeyringPair } from "@polkadot/keyring/types";
import type { ISubmittableResult } from "@polkadot/types/types";
import type { Header } from "@polkadot/types/interfaces";
import { blake2AsHex } from "@polkadot/util-crypto";
import { createKeyringPair } from "../../lib/runtimeUpgradeUtils";
import { DEFAULT_WS, preDigests, run } from "./common";

/** AURA key prefix → local-env container index. */
const NODE_BY_AURA: Record<string, number> = {
  "0xd435": 1,
  "0x8eaf": 2,
  "0x90b5": 3,
  "0xe855": 4,
  "0xb0eb": 5,
};
const WEIGHT = { refTime: 10_000_000_000, proofSize: 65_536 };

const log = (msg: string) =>
  console.log(`${new Date().toISOString().slice(11, 19)} ${msg}`);
const sh = (cmd: string) =>
  exec(cmd, (e, out, err) =>
    log(`  $ ${cmd} → ${e ? `ERR ${err.trim()}` : out.trim()}`),
  );

function submit(
  api: ApiPromise,
  tx: SubmittableExtrinsic,
  signer: KeyringPair,
  label: string,
): Promise<ISubmittableResult> {
  return new Promise((resolve, reject) => {
    tx.signAndSend(signer, { nonce: -1 }, (r) => {
      if (r.dispatchError) {
        const e = r.dispatchError.isModule
          ? api.registry.findMetaError(r.dispatchError.asModule)
          : null;
        reject(
          new Error(
            `${label}: ${e ? `${e.section}.${e.name}` : r.dispatchError}`,
          ),
        );
      } else if (r.status.isInBlock) resolve(r);
    }).catch(reject);
  });
}

async function approve(
  api: ApiPromise,
  name: "council" | "technicalCommittee",
  call: SubmittableExtrinsic["method"],
  lengthBound: number,
  members: KeyringPair[],
) {
  const threshold = Math.ceil(
    (((await api.query[name].members()).toJSON() as string[]).length * 2) / 3,
  );
  const r = await submit(
    api,
    api.tx[name].propose(threshold, call, lengthBound),
    members[0],
    `${name}.propose`,
  );
  const proposed = r.events.find(
    ({ event }) => event.section === name && event.method === "Proposed",
  );
  if (!proposed) throw new Error(`${name}: no Proposed event`);
  const ev = proposed.event;
  const [index, hash] = [
    ev.data[1].toPrimitive() as number,
    ev.data[2].toHex(),
  ];
  for (const m of members)
    await submit(api, api.tx[name].vote(hash, index, true), m, `${name}.vote`);
  await submit(
    api,
    api.tx[name].close(
      hash,
      index,
      api.createType("WeightV2", WEIGHT),
      lengthBound,
    ),
    members[0],
    `${name}.close`,
  );
  log(`${name}: proposal ${index} approved`);
}

const slotOf = (h: Header): bigint => {
  const pre = preDigests(h);
  const slot = pre.auraSlot ?? pre.babe?.slot;
  if (slot === undefined) throw new Error(`#${h.number} has no slot digest`);
  return slot;
};

/** AURA authority key prefixes (as hex) in authority-index order. */
const auraKeys = (codec: { toJSON(): unknown }) =>
  (codec.toJSON() as string[]).map((k) => k.slice(0, 6));

void run(process.argv[2] ?? DEFAULT_WS, async (api) => {
  const engine = async (hash?: string) =>
    (
      await (
        hash ? await api.at(hash) : api
      ).query.consensusEngine.engineState()
    ).toString();
  if ((await engine()) !== "Aura") throw new Error("engineState must be Aura");
  if (((await api.query.babe.authorities()).toJSON() as unknown[]).length === 0)
    throw new Error("babe.authorities is empty — wait for a rotation");

  const spe = BigInt(api.consts.babe.epochDuration.toString());
  const keys = (uris: string[]) => uris.map((u) => createKeyringPair(u, u));
  const inner = api.tx.consensusEngine.scheduleFlip();
  const motion = api.tx.federatedAuthority.motionApprove(inner.method);
  const lengthBound = motion.method.encodedLength;
  await approve(
    api,
    "council",
    motion.method,
    lengthBound,
    keys(["//Four", "//Five", "//Six"]),
  );
  await approve(
    api,
    "technicalCommittee",
    motion.method,
    lengthBound,
    keys(["//One", "//Two", "//Three"]),
  );

  let closeAt: number | undefined;
  let closeSent = false;
  let stopped:
    | { node: number; lastSlot: bigint; restarted?: boolean }
    | undefined;
  let prev = -1n;

  await new Promise<void>((resolve, reject) => {
    void api.rpc.chain
      .subscribeNewHeads(async (h) => {
        const n = h.number.toNumber();
        const slot = slotOf(h);
        const pos = slot % spe;
        const state = await engine(h.hash.toHex());
        // the slot owner comes from the parent's authorities (a new session swaps them in this block)
        const parent = auraKeys(
          await api.query.aura.authorities.at(h.parentHash),
        );
        const author = parent[Number(slot % BigInt(parent.length))];
        const gap =
          prev >= 0n && slot - prev > 1n
            ? ` (skipped slots ${prev + 1n}..${slot - 1n})`
            : "";
        prev = slot;
        log(
          `#${n} slot=${slot} pos=${pos}/${spe - 1n} author=node-${NODE_BY_AURA[author] ?? author} engine=${state}${gap}`,
        );

        // (c) close right after the block before the last slot, so it lands in the last slot
        if (!closeSent && pos === spe - 2n) {
          closeSent = true;
          const tx = api.tx.federatedAuthority.motionClose(
            blake2AsHex(inner.method.toU8a()),
            api.createType("WeightV2", WEIGHT),
          );
          submit(api, tx, createKeyringPair("//One", "executor"), "motionClose")
            .then(async (r) => {
              const hd = await api.rpc.chain.getHeader(r.status.asInBlock);
              closeAt = hd.number.toNumber();
              log(
                `  motionClose in #${closeAt} pos=${slotOf(hd) % spe} engine-after=${await engine(hd.hash.toHex())}`,
              );
            })
            .catch(reject);
        }
        // (a) first block after scheduling: stop the owner of this epoch's last slot
        const current = auraKeys(
          await (await api.at(h.hash)).query.aura.authorities(),
        );
        if (
          state === "ScheduledFlip" &&
          closeAt !== undefined &&
          n > closeAt &&
          !stopped
        ) {
          const lastSlot = slot - pos + spe - 1n;
          const node =
            NODE_BY_AURA[current[Number(lastSlot % BigInt(current.length))]];
          stopped = { node, lastSlot };
          log(`  (a) last slot ${lastSlot} owned by node-${node}: stopping it`);
          sh(`docker stop -t 2 midnight-node-${node}`);
        }
        if (stopped && !stopped.restarted && slot > stopped.lastSlot) {
          stopped.restarted = true;
          log(
            `  last slot ${stopped.lastSlot} passed with engine=${state}: restarting node-${stopped.node}`,
          );
          sh(`docker start midnight-node-${stopped.node}`);
        }
        if (state === "Babe") {
          const g = await (await api.at(h.hash)).query.babe.genesisSlot();
          log(`  FLIP in #${n} slot=${slot} pos=${pos}; babe.genesisSlot=${g}`);
          resolve();
        }
      })
      .catch(reject);
  });
});
