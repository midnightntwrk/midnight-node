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

// Shared by the drivers that time a scheduleFlip motion against slots (postpone, halt).

import { exec } from "child_process";
import type { ApiPromise } from "@polkadot/api";
import type { SubmittableExtrinsic } from "@polkadot/api/promise/types";
import type { KeyringPair } from "@polkadot/keyring/types";
import type { ISubmittableResult } from "@polkadot/types/types";
import type { Header } from "@polkadot/types/interfaces";
import { blake2AsHex } from "@polkadot/util-crypto";
import { createKeyringPair } from "../../lib/runtimeUpgradeUtils";
import { preDigests } from "./common";

const WEIGHT = { refTime: 10_000_000_000, proofSize: 65_536 };

export const log = (msg: string) =>
  console.log(`${new Date().toISOString().slice(11, 19)} ${msg}`);

/** Run a shell command without blocking the head subscription; log its result. */
export const sh = (cmd: string) =>
  exec(cmd, (e, out, err) =>
    log(`  $ ${cmd} → ${e ? `ERR ${err.trim()}` : out.trim()}`),
  );

/** Slot from the AURA pre-digest, else the BABE one. */
export const slotOf = (h: Header): bigint => {
  const pre = preDigests(h);
  const slot = pre.auraSlot ?? pre.babe?.slot;
  if (slot === undefined) throw new Error(`#${h.number} has no slot digest`);
  return slot;
};

export const engineState = async (api: ApiPromise, hash?: string) =>
  (
    await (hash ? await api.at(hash) : api).query.consensusEngine.engineState()
  ).toString();

export function submit(
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

/**
 * Approve a federated `consensusEngine.scheduleFlip` motion in both collectives (local-env
 * dev keys) and return a function that closes it — i.e. dispatches the call — when the
 * caller decides. Resolves to the header of the block the close landed in.
 */
export async function approveScheduleFlip(
  api: ApiPromise,
): Promise<() => Promise<Header>> {
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
  return async () => {
    const tx = api.tx.federatedAuthority.motionClose(
      blake2AsHex(inner.method.toU8a()),
      api.createType("WeightV2", WEIGHT),
    );
    const r = await submit(
      api,
      tx,
      createKeyringPair("//One", "executor"),
      "motionClose",
    );
    return api.rpc.chain.getHeader(r.status.asInBlock);
  };
}
