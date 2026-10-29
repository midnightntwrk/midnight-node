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

// An opt-in live node test: bridge is inactive, so this changes only the dev signer nonce.
import assert from 'node:assert/strict';
import { ApiPromise, WsProvider, Keyring } from '@polkadot/api';
import { mkdirSync, readFileSync, existsSync } from 'node:fs';
import { join } from 'node:path';
import { spawn } from 'node:child_process';
import { beforeSubmission, observedSubmission, atomicJson } from './manual-control.mjs';
assert.equal(process.env.SNOWBRIDGE_MANUAL,'1');
const dir=process.env.SNOWBRIDGE_PROOF_DIR;
assert(dir);mkdirSync(dir,{recursive:true});
const api=await ApiPromise.create({provider:new WsProvider(`ws://127.0.0.1:${process.env.SNOWBRIDGE_NODE_PORT ?? 19944}`),
  signedExtensions:{AuthorizeCall:{extrinsic:{},payload:{}},CheckCallFilter:{extrinsic:{},payload:{}},CheckThrottle:{extrinsic:{},payload:{}},WeightReclaim:{extrinsic:{},payload:{}}}});
const keyring=new Keyring({type:'sr25519'});
const alice=keyring.addFromUri('//Alice'),bob=keyring.addFromUri('//Bob');
let viewer;
try {
  const tx=api.tx.ethereumBridge.exportRedemption(0);
  const waiting=beforeSubmission(api,dir,tx,alice);
  // Simulate the operator's separately submitted call, not the gated sender's path.
  const result=await new Promise((resolve,reject)=>{
    let unsubscribe;
    tx.signAndSend(alice,{nonce:-1},r=>{if(r.status.isFinalized){unsubscribe?.();resolve(r);}}).then(fn=>unsubscribe=fn).catch(reject);
  });
  const request=JSON.parse(readFileSync(join(dir,'manual/pending.json')));
  const response={id:request.id,action:'submitted',blockHash:result.status.asFinalized.toHex(),extrinsicHash:tx.hash.toHex()};
  atomicJson(join(dir,'manual',`${request.id}.response.json`),response);
  const observed=await waiting;
  assert.equal(api.registry.findMetaError(observed.dispatchError.asModule).name,'Inactive');
  assert.equal(observed.transactionHash,tx.hash.toHex());
  await assert.rejects(observedSubmission(api,api.tx.ethereumBridge.exportRedemption(1),alice,response),/different call/);
  await assert.rejects(observedSubmission(api,tx,bob,response),/different signer/);
  await assert.rejects(observedSubmission(api,tx,null,response),/signedness/);
  viewer=spawn(process.execPath,['tests/snowbridge/manual.mjs','serve',dir],{env:{...process.env,SNOWBRIDGE_VIEW_PORT:'18082'},stdio:'inherit'});
  let snapshot;
  for(let i=0;i<30;i++) {
    try {snapshot=await (await fetch('http://127.0.0.1:18082/state')).json();if(snapshot.midnight) break;}catch{}
    await new Promise(resolve=>setTimeout(resolve,1000));
  }
  assert(snapshot?.midnight?.bridge);
  assert.equal(snapshot.midnight.bridge.registry,null);
  assert.equal(snapshot.midnight.bridge.deposits.length,0);
  assert.equal(snapshot.midnight.bridge.halted,false);
  const page=await (await fetch('http://127.0.0.1:18082')).text();
  assert(page.includes('Compact contracts'));
  assert.equal((await fetch('http://127.0.0.1:18082/state',{method:'POST'})).status,405);
  console.log('PASS: live manual gate, finalized event attribution, wrong call/signer rejection and read-only viewer');
} finally {
  if(viewer && viewer.exitCode===null) {viewer.kill('SIGTERM');await new Promise(resolve=>viewer.once('exit',resolve));}
  await api.disconnect();
}
