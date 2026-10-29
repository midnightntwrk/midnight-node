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

import assert from 'node:assert/strict';
import { existsSync, mkdirSync, readFileSync, renameSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { randomUUID } from 'node:crypto';
import { decodeAddress } from '@polkadot/util-crypto';

export const manualEnabled = () => process.env.SNOWBRIDGE_MANUAL === '1';
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
export function atomicJson(path, value) {
  const temp = `${path}.${randomUUID()}.tmp`;
  writeFileSync(temp, JSON.stringify(value, null, 2)+'\n');
  renameSync(temp, path);
}
export async function checkpoint(dir, title, detail = {}, enabled = manualEnabled()) {
  if (!enabled) return { action: 'continue' };
  const folder = join(dir, 'manual');
  mkdirSync(folder, { recursive: true });
  const request = { id: randomUUID(), title, createdAt: new Date().toISOString(), ...detail };
  atomicJson(join(folder, `${request.id}.request.json`), request);
  atomicJson(join(folder, 'pending.json'), request);
  console.log(`MANUAL PAUSE: ${title} — node tests/snowbridge/manual.mjs status ${dir}`);
  const responsePath = join(folder, `${request.id}.response.json`);
  while (!existsSync(responsePath)) await sleep(250);
  const response = JSON.parse(readFileSync(responsePath));
  assert.equal(response.id, request.id, 'stale manual response');
  assert(['continue', 'submitted', 'abort'].includes(response.action), 'unknown manual response');
  atomicJson(join(folder, 'pending.json'), { ...request, resolved: response.action });
  if (response.action === 'abort') throw new Error('Manual walkthrough aborted by operator');
  assert(response.action !== 'submitted' || detail.kind === 'extrinsic', 'only extrinsics accept a UI transaction');
  return response;
}

// A UI submission must match the requested call and signer in canonical finalized history.
// Returning its actual scoped events keeps the normal acceptance assertions unchanged.
export async function observedSubmission(api, tx, signer, response, timeoutMs = 180_000) {
  assert.match(response.blockHash ?? '', /^0x[0-9a-fA-F]{64}$/);
  assert.match(response.extrinsicHash ?? '', /^0x[0-9a-fA-F]{64}$/);
  const block = await api.rpc.chain.getBlock(response.blockHash);
  const number = block.block.header.number;
  const deadline = Date.now() + timeoutMs;
  while (true) {
    const finalized = await api.rpc.chain.getHeader(await api.rpc.chain.getFinalizedHead());
    if (BigInt(finalized.number.toString()) >= BigInt(number.toString())) break;
    assert(Date.now() < deadline, 'UI submission did not finalize');
    await sleep(1000);
  }
  assert.equal((await api.rpc.chain.getBlockHash(number)).toHex(), response.blockHash, 'UI block is not canonical');
  const index = block.block.extrinsics.findIndex(item => item.hash.toHex() === response.extrinsicHash);
  assert(index >= 0, 'UI extrinsic not found in block');
  const actual = block.block.extrinsics[index];
  assert.equal(actual.method.toHex(), tx.method.toHex(), 'UI submitted a different call');
  assert.equal(actual.isSigned, Boolean(signer), 'UI signedness does not match');
  if (signer) assert.deepEqual(Buffer.from(decodeAddress(actual.signer.toString())), Buffer.from(signer.publicKey), 'UI used a different signer');
  const at = await api.at(response.blockHash);
  const events = (await at.query.system.events()).filter(({ phase }) => phase.isApplyExtrinsic && phase.asApplyExtrinsic.toNumber() === index);
  const failure = events.find(({ event }) => event.section === 'system' && event.method === 'ExtrinsicFailed');
  assert(failure || events.some(({ event }) => event.section === 'system' && event.method === 'ExtrinsicSuccess'), 'missing dispatch outcome');
  return { events, dispatchError: failure?.event.data[0], status: { asFinalized: api.registry.createType('Hash', response.blockHash) }, transactionHash: actual.hash.toHex() };
}

export async function beforeSubmission(api, dir, tx, signer) {
  const response = await checkpoint(dir, `Submit ${tx.method.section}.${tx.method.method}`, {
    kind: 'extrinsic', call: `${tx.method.section}.${tx.method.method}`,
    args: tx.method.args.map(value => value.toJSON()), callHex: tx.method.toHex(),
    unsignedExtrinsicHex: tx.toHex(), signer: signer?.address ?? null,
    signerPublicKey: signer ? '0x'+Buffer.from(signer.publicKey).toString('hex') : null,
  });
  return response.action === 'submitted' ? observedSubmission(api, tx, signer, response) : null;
}
