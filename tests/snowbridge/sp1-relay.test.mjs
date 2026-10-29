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

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, writeFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { encodeCheckpoint, recoverJustifications, recoverCommitteeProofs, hasNextCommitteeSnapshot } from './sp1-relay.mjs';

test('checkpoint SCALE has exact lengths, little endian IDs and canonical Option tags',()=>{
  const set={id:0x102030405n,len:3,root:{toHex:()=> '0x'+'11'.repeat(32)}};
  const noNext=encodeCheckpoint('0x'+'22'.repeat(32),0x10203,set,null);
  assert.equal(noNext.length,81);assert.equal(noNext[80],0);
  assert.equal(noNext.readUInt32LE(32),0x10203);assert.equal(noNext.readBigUInt64LE(36),set.id);
  const next=encodeCheckpoint('0x'+'22'.repeat(32),42,set,{...set,id:set.id+1n});
  assert.equal(next.length,125);assert.equal(next[80],1);assert.equal(next.readBigUInt64LE(81),set.id+1n);
});

test('rotation target waits for runtime confirmation and rejects unrelated committee IDs',()=>{
  const leaf=Buffer.alloc(113), next=Buffer.alloc(44); leaf[0]=0x20;
  const hex=b=>'0x'+b.toString('hex');
  next.writeBigUInt64LE(9n); next.writeUInt32LE(4,8);
  leaf.writeBigUInt64LE(8n,37); leaf.writeUInt32LE(4,45);
  assert.equal(hasNextCommitteeSnapshot(hex(leaf),hex(next),8n),false);
  leaf.writeBigUInt64LE(9n,37);
  assert.equal(hasNextCommitteeSnapshot(hex(leaf),hex(next),8n),true);
  leaf.writeBigUInt64LE(7n,37);
  assert.throws(()=>hasNextCommitteeSnapshot(hex(leaf),hex(next),8n),/unexpected runtime committee/);
  leaf.writeBigUInt64LE(9n,37); leaf.writeUInt32LE(3,45);
  assert.throws(()=>hasNextCommitteeSnapshot(hex(leaf),hex(next),8n),/count mismatch/);
});

test('committee recovery pins RPC requests to signed blocks and rejects stale or non-tip leaves',async()=>{
  const dir=mkdtempSync(join(tmpdir(),'sp1-committee-'));
  try {
    const initial=Buffer.alloc(81); initial.writeUInt32LE(3,32); writeFileSync(join(dir,'sp1-initial.scale'),initial);
    writeFileSync(join(dir,'beefy-5.scale'),Buffer.from([1]));
    writeFileSync(join(dir,'beefy-8.scale'),Buffer.from([1]));
    const leaf=Buffer.alloc(113); leaf[0]=0x20; leaf.writeUInt32LE(4,1);
    const proof={leafIndices:[4n],leafCount:5n,items:[]};
    const api={rpc:{chain:{getBlockHash:async n=>({toHex:()=>`hash-${n}`})}},
      registry:{createType:type=>type==='Vec<Bytes>'?[{toU8a:()=>leaf}]:proof}};
    const provider={send:async(method,args)=>{
      assert.equal(method,'mmr_generateProof'); assert.deepEqual(args,[[5],5,'hash-5']); return {};
    }};
    leaf.writeUInt32LE(3,1);
    await assert.rejects(recoverCommitteeProofs(api,provider,dir,5),/historical/);
    assert(!existsSync(join(dir,'committee-5.json')));
    leaf.writeUInt32LE(4,1); proof.leafIndices=[3n];
    await assert.rejects(recoverCommitteeProofs(api,provider,dir,5),/not the MMR tip/);
    proof.leafIndices=[4n];
    await recoverCommitteeProofs(api,provider,dir,5);
    const saved=JSON.parse(readFileSync(join(dir,'committee-5.json')));
    assert.equal(saved.blockHash,'hash-5'); assert.equal(saved.leafCount,'5');
    assert(!existsSync(join(dir,'committee-8.json')));
    provider.send=async()=>{throw Error('must reuse saved witness');};
    await recoverCommitteeProofs(api,provider,dir,5);
  } finally {rmSync(dir,{recursive:true,force:true});}
});
test('reconnect backfill persists finalized BEEFY witnesses and does not advance on RPC failure',async()=>{
  const dir=mkdtempSync(join(tmpdir(),'sp1-backfill-'));
  try {
    const initial=Buffer.alloc(81);initial.writeUInt32LE(3,32);writeFileSync(join(dir,'sp1-initial.scale'),initial);
    const api={rpc:{chain:{getFinalizedHead:async()=> 'final',getHeader:async()=>({number:{toNumber:()=>5}}),getBlockHash:async n=>({toHex:()=>String(n)})}}};
    const provider={send:async(_, [n])=>{if(n==='5')throw Error('disconnect');return {justifications:[['0x42454546','0x010203'],['0x46524e4b','0x99']]};}};
    await assert.rejects(recoverJustifications(api,provider,dir,9),/disconnect/);
    assert(!existsSync(join(dir,'sp1-backfill.json')));
    provider.send=async()=>({justifications:[[[66,69,69,70],[1,2,3]]]});
    await recoverJustifications(api,provider,dir,9);
    assert.equal(JSON.parse(readFileSync(join(dir,'sp1-backfill.json'))).block,5);
    assert.equal(readFileSync(join(dir,'beefy-4.scale')).toString('hex'),'010203');
    assert.equal(readFileSync(join(dir,'beefy-5.scale')).toString('hex'),'010203');
    assert(!existsSync(join(dir,'beefy-6.scale')));
  } finally {rmSync(dir,{recursive:true,force:true});}
});
