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
import { readFileSync, writeFileSync, renameSync, existsSync, mkdirSync, readdirSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { Contract, JsonRpcProvider, Wallet, keccak256, concat } from 'ethers';
const ROOT = resolve(import.meta.dirname, '../..');
const ABI = ['function checkpoint() view returns(bytes)', 'function checkpointHash() view returns(bytes32)',
  'function latestBeefyBlock() view returns(uint32)', 'function latestMMRRoot() view returns(bytes32)',
  'function programVKey() view returns(bytes32)', 'function verifier() view returns(address)',
  'function submitProof(bytes proofBytes,bytes publicValues)'];
const hex = b => '0x'+Buffer.from(b).toString('hex');
function rpcBytes(value) {
  if (typeof value === 'string') {
    assert(/^0x(?:[0-9a-fA-F]{2})*$/.test(value),'invalid RPC hex bytes');
    return Buffer.from(value.slice(2),'hex');
  }
  assert(Array.isArray(value) && value.every(n=>Number.isInteger(n)&&n>=0&&n<=255),'invalid RPC byte array');
  return Buffer.from(value);
}
function atomic(path, value) {
  writeFileSync(path+'.tmp', JSON.stringify(value, null, 2)+'\n'); renameSync(path+'.tmp',path);
}
export function encodeCheckpoint(genesis, block, current, next) {
  const number = (n,size) => { const b=Buffer.alloc(size); if(size===8)b.writeBigUInt64LE(BigInt(n));else b.writeUInt32LE(Number(n)); return b; };
  const set = s => Buffer.concat([number(s.id.toString(),8),number(s.len.toString(),4),Buffer.from((s.keysetCommitment??s.root).toHex().slice(2),'hex')]);
  return Buffer.concat([Buffer.from(genesis.slice(2),'hex'),number(block,4),set(current),Buffer.from([next?1:0]),...(next?[set(next)]:[])]);
}
// Backfill consensus proofs from finalized source blocks after a listener gap/restart.
// These are witnesses; the guest still authenticates every transition from destination state.
export async function recoverJustifications(api, provider, dir, target) {
  const cursorPath=join(dir,'sp1-backfill.json');
  const initial=readFileSync(join(dir,'sp1-initial.scale')).readUInt32LE(32);
  let cursor=existsSync(cursorPath)?JSON.parse(readFileSync(cursorPath)).block:initial;
  const finalHeader=await api.rpc.chain.getHeader(await api.rpc.chain.getFinalizedHead());
  const end=Math.min(target,finalHeader.number.toNumber());
  while(cursor<end) {
    const numbers=Array.from({length:Math.min(32,end-cursor)},(_,i)=>cursor+i+1);
    await Promise.all(numbers.map(async number=>{
      const path=join(dir,`beefy-${number}.scale`);
      if(existsSync(path)) return;
      const hash=(await api.rpc.chain.getBlockHash(number)).toHex();
      const signed=await provider.send('chain_getBlock',[hash]);
      assert(signed && typeof signed === 'object','historical source block unavailable');
      for(const [engine,proof] of signed.justifications??[]) {
        if(rpcBytes(engine).equals(Buffer.from('BEEF'))) {
          writeFileSync(path+'.tmp',rpcBytes(proof)); renameSync(path+'.tmp',path);
        }
      }
    }));
    cursor=numbers.at(-1); atomic(cursorPath,{block:cursor});
  }
}

// Fetch the tip leaf at each captured commitment, never a latest-state authority list.
// RPC data is only a witness: the SP1 guest checks inclusion against the signed root.
export async function recoverCommitteeProofs(api, provider, dir, target) {
  const initial=readFileSync(join(dir,'sp1-initial.scale')).readUInt32LE(32);
  const blocks=readdirSync(dir).map(name=>/^beefy-(\d+)\.scale$/.exec(name))
    .filter(Boolean).map(match=>Number(match[1])).filter(block=>block>initial&&block<=target).sort((a,b)=>a-b);
  for(const block of blocks) {
    const path=join(dir,`committee-${block}.json`);
    if(existsSync(path)) continue;
    const hash=(await api.rpc.chain.getBlockHash(block)).toHex();
    const result=await provider.send('mmr_generateProof',[[block],block,hash]);
    const leaves=api.registry.createType('Vec<Bytes>',result.leaves);
    const proof=api.registry.createType('MmrBatchProof',result.proof);
    assert.equal(leaves.length,1,'one committee tip leaf required');
    assert.equal(proof.leafIndices.length,1,'one committee tip index required');
    const leaf=Buffer.from(leaves[0].toU8a(true));
    assert.equal(leaf.length,113); assert.equal(leaf[0],0x20);
    assert.equal(leaf.readUInt32LE(1)+1,block,'historical committee leaf');
    assert.equal(BigInt(proof.leafIndices[0].toString())+1n,BigInt(proof.leafCount.toString()),'not the MMR tip');
    atomic(path,{block,blockHash:hash,leaf:hex(leaf),leafIndex:proof.leafIndices[0].toString(),
      leafCount:proof.leafCount.toString(),items:proof.items.map(item=>item.toHex())});
  }
}

// Scheduling hint only; the guest still verifies signatures, roots and membership.
export function hasNextCommitteeSnapshot(leafHex, nextSetHex, currentId) {
  const leaf=rpcBytes(leafHex), next=rpcBytes(nextSetHex);
  assert.equal(leaf.length,113); assert.equal(leaf[0],0x20); assert.equal(next.length,44);
  const current=BigInt(currentId), announced=next.readBigUInt64LE(0);
  assert.equal(announced,current+1n,'invalid successor ID');
  const committed=leaf.readBigUInt64LE(37);
  if(committed===current) return false; // Runtime MMR snapshot precedes rotation.
  assert.equal(committed,announced,'unexpected runtime committee ID');
  assert.equal(leaf.readUInt32LE(45),next.readUInt32LE(8),'runtime committee count mismatch');
  return true;
}

export async function proveAndRelay(dir, block) {
  const network = JSON.parse(readFileSync(join(dir,'network.json')));
  const deployment = JSON.parse(readFileSync(join(dir,'deployment.json')));
  assert.equal(deployment.sp1,true);
  const provider = new JsonRpcProvider(network.execution);
  const signer = new Wallet('0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80',provider);
  const client = new Contract(deployment.beefyClient,ABI,signer);
  const work = join(dir,'sp1'); mkdirSync(work,{recursive:true});
  const journalPath = join(work,`job-${block}.json`);
  let journal = existsSync(journalPath) ? JSON.parse(readFileSync(journalPath)) : null;
  const acceptedHash = await client.checkpointHash();
  // Reconcile an interrupted broadcast against actual chain state before creating more work.
  if (journal?.phase === 'Submitted' && acceptedHash === journal.previous) {
    if (!(await provider.getTransaction(journal.transaction))) {
      await provider.broadcastTransaction(journal.rawTransaction);
    }
    return await finish(journal);
  }
  if (journal?.proof && acceptedHash === journal.proof.checkpointHash) {
    assert(journal.transaction, 'accepted job has no recorded transaction');
    return await finish(journal);
  }
  const state = await client.checkpoint();
  const previous = keccak256(concat([Buffer.from('midnight-vector-state-v1'),state]));
  assert.equal(previous,acceptedHash);
  assert(Number(await client.latestBeefyBlock()) < block,'destination already advanced past target');
  const out = journal?.previous === previous && journal.out && existsSync(join(journal.out,'proof.json'))
    ? journal.out : join(work,`${block}-${previous.slice(2,14)}`);
  const checkpointPath = join(work,`prior-${block}.scale`);
  writeFileSync(checkpointPath,Buffer.from(state.slice(2),'hex'));
  if (!existsSync(join(out,'proof.json'))) {
    // An incomplete prover directory remains evidence; a fresh suffix permits safe retry.
    const jobOut = existsSync(out) ? out+'-'+Date.now() : out;
    atomic(journalPath,{phase:'Proving',block,previous,out:jobOut});
    const log = join(work,`prove-${block}.log`);
    try {
      const result=await promisify(execFile)(join(ROOT,'artifacts/snowbridge/sp1-target/release/midnight-sp1-prover'),
        ['--checkpoint',checkpointPath,'--captures',dir,'--block',String(block),'--out',jobOut],
        {cwd:ROOT,env:process.env,timeout:7_200_000,maxBuffer:32*1024*1024});
      writeFileSync(log,result.stdout+result.stderr);
    } catch(e) { writeFileSync(log,(e.stdout??'')+(e.stderr??'')+String(e)); throw e; }
    journal={phase:'Proved',block,previous,out:jobOut,proof:JSON.parse(readFileSync(join(jobOut,'proof.json')))};
  } else journal={phase:'Proved',block,previous,out,proof:JSON.parse(readFileSync(join(out,'proof.json')))};
  const proof=journal.proof;
  assert.equal(proof.previousStateHash,previous);
  assert.equal(proof.programVKey,await client.programVKey());
  assert.equal(proof.block,block); assert.equal(proof.realGroth16,true);
  assert.equal(proof.committeeStateVerified,true,'proof predates runtime committee checks');
  atomic(journalPath,journal);
  // Real verifier negative checks, executed against the same on-chain receiver.
  const flip=(value,index)=>{const b=Buffer.from(value.slice(2),'hex');b[index]^=1;return hex(b);};
  await assert.rejects(client.submitProof.staticCall(flip(proof.proofBytes,(proof.proofBytes.length-2)/2-1),proof.publicValues));
  await assert.rejects(client.submitProof.staticCall(proof.proofBytes,flip(proof.publicValues,proof.publicValues.length/2-8)));
  await assert.rejects(client.submitProof.staticCall(proof.proofBytes,flip(proof.publicValues,0)));
  await assert.rejects(client.submitProof.staticCall(proof.proofBytes,flip(proof.publicValues,32)));
  const verifier = new Contract(await client.verifier(),['function verifyProof(bytes32,bytes,bytes) view'],provider);
  await assert.rejects(verifier.verifyProof(flip(proof.programVKey,0),proof.publicValues,proof.proofBytes));
  await client.submitProof.staticCall(proof.proofBytes,proof.publicValues);
  assert.equal(await client.checkpointHash(),previous);
  // Sign and persist the transaction hash BEFORE broadcasting; recovery can rebroadcast identical bytes.
  const request=await client.submitProof.populateTransaction(proof.proofBytes,proof.publicValues);
  const raw=await signer.signTransaction(await signer.populateTransaction({...request,gasLimit:1_000_000}));
  journal.rawTransaction=raw; journal.transaction=keccak256(raw); journal.phase='Submitted';
  atomic(journalPath,journal);
  await provider.broadcastTransaction(raw);
  return await finish(journal);

  async function finish(job) {
    const deadline=Date.now()+900_000;
    let receipt;
    while(Date.now()<deadline) {
      receipt=await provider.getTransactionReceipt(job.transaction);
      if(receipt) {
        assert.equal(receipt.status,1,'SP1 submission reverted');
        const finalized=await provider.send('eth_getBlockByNumber',['finalized',false]);
        if(finalized && BigInt(finalized.number)>=BigInt(receipt.blockNumber)) break;
      }
      await new Promise(r=>setTimeout(r,4000));
    }
    assert(receipt,'SP1 submission receipt missing');
    const finalized=await provider.send('eth_getBlockByNumber',['finalized',false]);
    assert(BigInt(finalized.number)>=BigInt(receipt.blockNumber),'SP1 submission not finalized');
    assert.equal((await provider.getBlock(receipt.blockNumber)).hash,receipt.blockHash);
    assert.equal(await client.checkpointHash(),job.proof.checkpointHash);
    assert.equal(await client.latestMMRRoot(),job.proof.mmrRoot);
    assert.equal(Number(await client.latestBeefyBlock()),block);
    await assert.rejects(client.submitProof.staticCall(job.proof.proofBytes,job.proof.publicValues));
    job.phase='Finalized'; job.receipt=receipt.toJSON(); job.negativeChecks=6;
    job.verifierCodeHash=keccak256(await provider.getCode(await client.verifier()));
    job.receiverCodeHash=keccak256(await provider.getCode(deployment.beefyClient));
    atomic(journalPath,job);
    atomic(join(work,'destination.json'),{block,checkpointHash:job.proof.checkpointHash,checkpoint:job.proof.checkpoint,transaction:job.transaction});
    provider.destroy();
    return {block,transaction:job.transaction,checkpointHash:job.proof.checkpointHash,mmrRoot:job.proof.mmrRoot,gasUsed:receipt.gasUsed.toString(),realGroth16:true,finalized:true};
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  assert(process.argv[2] && /^\d+$/.test(process.argv[3]??''),'usage: sp1-relay.mjs RUN_DIRECTORY TARGET_BLOCK');
  console.log(JSON.stringify(await proveAndRelay(resolve(process.argv[2]),Number(process.argv[3])),null,2));
}
