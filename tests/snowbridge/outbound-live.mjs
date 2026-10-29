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
import { readFileSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { keccakAsHex, secp256k1Expand } from '@polkadot/util-crypto';
import { encodeCheckpoint, recoverJustifications, recoverCommitteeProofs, hasNextCommitteeSnapshot, proveAndRelay } from './sp1-relay.mjs';
import { checkpoint } from './manual-control.mjs';
const ROOT=resolve(import.meta.dirname,'../..');
const ART=join(ROOT,'artifacts/snowbridge');
const sleep=ms=>new Promise(resolve=>setTimeout(resolve,ms));
const hex=bytes=>'0x'+Buffer.from(bytes).toString('hex');
const json=(path,value)=>writeFileSync(path,JSON.stringify(value,null,2)+'\n');
async function run(dir,name,command,args,extra={},cwd=ROOT) {
  await checkpoint(dir,`Run helper: ${name}`,{kind:'helper',command,args,cwd,environment:extra});
  console.log(`Running ${name}`);
  try {
    const {stdout,stderr}=await promisify(execFile)(command,args,{cwd,env:{...process.env,...extra},timeout:1_800_000,maxBuffer:32*1024*1024});
    writeFileSync(join(dir,`${name}.log`),stdout+stderr);
    return stdout;
  } catch(error) {
    writeFileSync(join(dir,`${name}.log`),(error.stdout??'')+(error.stderr??'')+String(error));
    throw error;
  }
}
async function eth(endpoint,method,params=[]) {
  const response=await fetch(endpoint,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({jsonrpc:'2.0',id:1,method,params})});
  const result=await response.json();
  assert(!result.error,JSON.stringify(result.error));
  return result.result;
}
function authority(value) {
  return {id:Number(value.id.toString()),len:Number(value.len.toString()),root:(value.keysetCommitment ?? value.root).toHex()};
}
const captureSubscriptions = new Map();
export async function prepareLiveBridge(api,provider,dir) {
  if (process.env.SNOWBRIDGE_SP1 === 'true') {
    // Capture announcements during ingress/proving, before any session transition is missed.
    const subscription = api.rpc.beefy.subscribeJustifications(proof => {
      assert(proof.isV1);
      writeFileSync(join(dir,`beefy-${proof.asV1.commitment.blockNumber}.scale`),proof.toU8a());
      json(join(dir,'sp1-observed.json'),{block:proof.asV1.commitment.blockNumber.toNumber(),setId:proof.asV1.commitment.validatorSetId.toString()});
    });
    captureSubscriptions.set(dir,()=>subscription.then(unsubscribe=>unsubscribe()));
  }
  const network=JSON.parse(readFileSync(join(dir,'network.json')));
  const finalizedHash=await api.rpc.chain.getFinalizedHead();
  const at=await api.at(finalizedHash);
  const current=authority(await at.call.beefyMmrApi.authoritySetProof());
  const next=authority(await at.call.beefyMmrApi.nextAuthoritySetProof());
  assert.equal(current.len,1,'this local profile expects one live authority');
  const checkpoint={current,next,midnightGenesis:api.genesisHash.toHex(),
    ethereumGenesis:(await eth(network.execution,'eth_getBlockByNumber',['0x0',false])).hash,
    midnightGenesisLedgerRoot:await provider.send('midnight_ledgerStateRoot',[api.genesisHash.toHex()])};
  if (process.env.SNOWBRIDGE_SP1 === 'true') {
    // This local profile has one fixed stake-one authority in both initial sets.
    // Anchor its key against BOTH independent address-only MMR authority roots.
    const keys=await at.query.beefy.authorities();
    assert.equal(keys.length,1);
    const address=Buffer.from(keccakAsHex(secp256k1Expand(keys[0].toU8a())).slice(-40),'hex');
    assert.equal(keccakAsHex(address),current.root);
    assert.equal(keccakAsHex(address),next.root);
    const stake=Buffer.alloc(8); stake.writeBigUInt64LE(1n);
    const root=keccakAsHex(Buffer.concat([address,stake]));
    const currentStake={...current,root:{toHex:()=>root}};
    const nextStake={...next,root:{toHex:()=>root}};
    const height=(await api.rpc.chain.getHeader(finalizedHash)).number.toNumber();
    const encoded=encodeCheckpoint(checkpoint.midnightGenesis,height,currentStake,nextStake);
    checkpoint.sp1Checkpoint=hex(encoded);
    writeFileSync(join(dir,'sp1-initial.scale'),encoded);
    const key=JSON.parse(readFileSync(process.env.SNOWBRIDGE_SP1_KEY));
    assert.equal(key.profile,'ethereum-address-stake-v2-committee-mmr','rebuild the SP1 guest for committee protection');
    json(join(dir,'sp1-key.json'),key);
    process.env.SNOWBRIDGE_SP1_KEY=join(dir,'sp1-key.json');
  }
  json(join(dir,'beefy-checkpoint.json'),checkpoint);
  process.env.SNOWBRIDGE_BEEFY_CHECKPOINT=join(dir,'beefy-checkpoint.json');
  await run(dir,'ingress-preparation','python3',['tests/snowbridge/pos-ingress.py']);
  await run(dir,'escrow-build','node',['tests/snowbridge/live-escrow.mjs']);
  await run(dir,'compact-build','node',['tests/snowbridge/live-compact.mjs']);
  await run(dir,'ledger-build','cargo',['test','--locked','--manifest-path','tests/snowbridge/rust/Cargo.toml','--features','compact-ledger','prove_live_escrow_from_existing_genesis','--','--ignored','--nocapture']);
}
async function balance(network,deployment,address,block='latest') {
  return BigInt(await eth(network.execution,'eth_call',[{to:deployment.token,data:'0x70a08231'+address.replace(/^0x/,'').padStart(64,'0')},block]));
}
async function relayProof(api,provider,dir,id,attempt,signed) {
  const binding=(await api.query.ethereumBridge.byNonce(attempt.nonce)).unwrap();
  const accepted=Number(binding.acceptedBlock.toString());
  const signedBlock=Number(signed.commitment.blockNumber.toString());
  assert(signedBlock>=accepted+1);
  const hash=(await api.rpc.chain.getBlockHash(signedBlock)).toHex();
  const mmr=await provider.send('mmr_generateProof',[[accepted+1],signedBlock,hash]);
  json(join(dir,`mmr-${attempt.index}.json`),mmr);
  const leaves=api.registry.createType('Vec<Bytes>',mmr.leaves);
  assert.equal(leaves.length,1);
  const bytes=Buffer.from(leaves[0].toU8a(true));
  assert.equal(bytes.length,113,'fixed-width standalone MMR leaf');
  const leaf={version:bytes[0],parentNumber:bytes.readUInt32LE(1),parentHash:hex(bytes.subarray(5,37)),
    nextAuthoritySetID:bytes.readBigUInt64LE(37).toString(),nextAuthoritySetLen:bytes.readUInt32LE(45),
    nextAuthoritySetRoot:hex(bytes.subarray(49,81)),parachainHeadsRoot:hex(bytes.subarray(81))};
  assert.equal(leaf.version,32); assert.equal(leaf.parentNumber,accepted);
  assert.equal(leaf.parentHash,(await api.rpc.chain.getBlockHash(accepted)).toHex());
  const historical=await api.at(leaf.parentHash);
  const messages=await historical.query.ethereumOutboundQueue.messages();
  assert.equal(messages.length,1,'local profile commits one message per block');
  const m=messages[0];
  assert.equal(m.nonce.toString(),String(attempt.nonce)); assert.equal(m.topic.toHex(),id);
  assert.equal(leaf.parachainHeadsRoot,binding.messageHash.toHex());
  const payload=signed.commitment.payload.map(([key,value])=>({payloadID:key.toHex(),data:value.toHex()}));
  const root=payload.find(p=>p.payloadID==='0x6d68')?.data; assert(root);
  const proof=api.registry.createType('MmrBatchProof',mmr.proof);
  assert.equal(proof.leafIndices.length,1);
  const input={leafIndex:Number(proof.leafIndices[0].toString()),leafCount:Number(proof.leafCount.toString()),items:proof.items.map(h=>h.toHex()),leafHash:keccakAsHex(bytes),root};
  json(join(dir,`mmr-input-${attempt.index}.json`),input);
  await run(dir,`mmr-convert-${attempt.index}`,join(ART,'tools/beacon-proof'),['--mode','mmr','--mmr-input',join(dir,`mmr-input-${attempt.index}.json`),'--out',join(dir,`mmr-simple-${attempt.index}`)]);
  const simple=JSON.parse(readFileSync(join(dir,`mmr-simple-${attempt.index}/simplified.json`)));
  const authorities=await (await api.at(hash)).query.beefy.authorities();
  assert.equal(authorities.length,1); assert.equal(signed.validatorSetLen.toNumber(),1);
  assert.equal(Number(signed.signaturesFrom[0])&128,128); assert.equal(signed.signaturesCompact.length,1);
  const expanded=secp256k1Expand(authorities[0].toU8a());
  const account='0x'+keccakAsHex(expanded).slice(-40);
  const sig=Buffer.from(signed.signaturesCompact[0].toU8a());
  assert.equal(sig.length,65);
  const result={payloadCount:payload.length,commitment:{blockNumber:signedBlock,validatorSetID:signed.commitment.validatorSetId.toString(),payload},
    validator:{v:sig[64]+27,r:hex(sig.subarray(0,32)),s:hex(sig.subarray(32,64)),index:0,account,proof:[]},
    leaf,leafProof:simple.items,leafProofOrder:simple.order,
    message:{origin:m.origin.toHex(),nonce:m.nonce.toString(),topic:m.topic.toHex(),commands:m.commands.map(c=>({kind:c.kind.toNumber(),gas:c.gas.toString(),payload:c.payload.toHex()}))}};
  json(join(dir,`outbound-proof-${attempt.index}.json`),result);
  return result;
}
export async function redeemLive({api,provider,dir,send,decodeArg,signer}) {
  const network=JSON.parse(readFileSync(join(dir,'network.json')));
  const deployment=JSON.parse(readFileSync(join(dir,'deployment.json')));
  const commitments=[];
  const unsubscribe=await api.rpc.beefy.subscribeJustifications(proof=>{
    assert(proof.isV1); commitments.push(proof.asV1);
    writeFileSync(join(dir,`beefy-${proof.asV1.commitment.blockNumber}.scale`),proof.toU8a());
  });
  const attempts=[];
  try {
    const exported=await send(api.tx.ethereumBridge.exportRedemption(0),signer);
    const id=exported.events.find(({event})=>event.section==='ethereumBridge'&&event.method==='RedemptionQueued').event.data[0].toHex();
    await assert.rejects(()=>send(api.tx.ethereumBridge.exportRedemption(0),signer),/AlreadyExported/);
    await assert.rejects(()=>send(api.tx.ethereumBridge.retryRedemption(id),signer),/NotRetryEligible/);
    for (let index=0;index<2;index++) {
      if(index) await send(api.tx.ethereumBridge.retryRedemption(id),signer);
      let record,nonce;
      const deadline=Date.now()+300_000;
      while(Date.now()<deadline) {
        record=(await api.query.ethereumBridge.redemptions(id)).unwrap();
        if(record.phase.isAwaitingResult) { nonce=record.phase.asAwaitingResult.toNumber(); break; }
        await sleep(1000);
      }
      assert(nonce,'queued retirement was not accepted');
      const attempt={index,nonce}; attempts.push(attempt);
      const binding=(await api.query.ethereumBridge.byNonce(nonce)).unwrap();
      let signed;
      const deferred=new Set();
      while(Date.now()<deadline) {
        signed=commitments.find(c=>Number(c.commitment.blockNumber.toString())>=Number(binding.acceptedBlock.toString())+1
          && !deferred.has(c.commitment.blockNumber.toNumber()));
        if(signed && deployment.sp1) {
          const block=signed.commitment.blockNumber.toNumber();
          await recoverCommitteeProofs(api,provider,dir,block);
          const witness=JSON.parse(readFileSync(join(dir,`committee-${block}.json`)));
          const next=signed.commitment.payload.find(([id])=>id.toHex()==='0x6e62');
          assert(next,'missing next committee payload');
          if(!hasNextCommitteeSnapshot(witness.leaf,next[1].toHex(),signed.commitment.validatorSetId.toString())) {
            console.log(`Deferring rotation block ${block} until the runtime MMR confirms its successor`);
            deferred.add(block); signed=undefined;
          }
        }
        if(signed) break;
        await sleep(1000);
      }
      assert(signed,'no live BEEFY signature covering the accepted message');
      await relayProof(api,provider,dir,id,attempt,signed);
      if (deployment.sp1) {
        console.log(`Generating and finalizing SP1 proof for block ${signed.commitment.blockNumber}`);
        await recoverJustifications(api,provider,dir,signed.commitment.blockNumber.toNumber());
        await recoverCommitteeProofs(api,provider,dir,signed.commitment.blockNumber.toNumber());
        attempt.sp1=await proveAndRelay(dir,signed.commitment.blockNumber.toNumber());
      }
      await run(dir,`release-${index}`,join(ART,'tools/foundry/forge'),['script','script/PosRelease.s.sol:PosRelease','--gas-estimate-multiplier','500','--no-dynamic-test-linking','--rpc-url',network.execution,'--broadcast','--slow',
        '--private-key','0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80','--priority-gas-price','1000000000','--with-gas-price','2000000000'],
        {SNOWBRIDGE_POS_DEPLOYMENT:join(dir,'deployment.json'),SNOWBRIDGE_OUTBOUND_PROOF:join(dir,`outbound-proof-${index}.json`),SNOWBRIDGE_FAIL_RELEASE:index===0?'true':'false'},join(ROOT,'tests/snowbridge/contracts'));
      const broadcast=JSON.parse(readFileSync(join(ROOT,'tests/snowbridge/contracts/broadcast/PosRelease.s.sol/31337/run-latest.json')));
      json(join(dir,`release-broadcast-${index}.json`),broadcast);
      const tx=broadcast.transactions.at(-1).hash;
      const receipt=await eth(network.execution,'eth_getTransactionReceipt',[tx]);
      assert.equal(BigInt(receipt.status),1n);
      const logs=gatewayDispatchLogs(receipt,deployment.gateway);
      assert.equal(logs.length,1);
      const recipient='0x'+'05'.repeat(20);
      assert.equal(await balance(network,deployment,deployment.custody),index===0?100n:60n);
      assert.equal(await balance(network,deployment,recipient),index===0?0n:40n);
      const proofDir=join(dir,`release-receipt-${index}`);
      await run(dir,`release-receipt-${index}`,join(ART,'tools/beacon-proof'),['--mode','receipt','--beacon',network.beacon,'--execution',network.execution,'--tx',tx,'--log-index',String(parseInt(logs[0].logIndex,16)),'--out',proofDir]);
      await send(api.tx.ethereumBeaconClient.submit(decodeArg(api.tx.ethereumBeaconClient.submit,join(proofDir,'update.scale'))),signer);
      const event=decodeArg(api.tx.ethereumBridge.submitRedemptionReceipt,join(proofDir,'event.scale'));
      await send(api.tx.ethereumBridge.submitRedemptionReceipt(event),signer);
      await assert.rejects(()=>send(api.tx.ethereumBridge.submitRedemptionReceipt(event),signer),/AlreadySeen/);
      const phase=(await api.query.ethereumBridge.redemptions(id)).unwrap().phase;
      assert(index===0?phase.isRetryEligible:phase.isCompleted);
      attempt.ethereumTransaction=tx; attempt.success=index===1; attempt.finalizedReceiptVerified=true;
      if(index) {
        const stale=decodeArg(api.tx.ethereumBridge.submitRedemptionReceipt,join(dir,'release-receipt-0/event.scale'));
        await assert.rejects(()=>send(api.tx.ethereumBridge.submitRedemptionReceipt(stale),signer),/AlreadySeen/);
        await assert.rejects(()=>send(api.tx.ethereumBridge.retryRedemption(id),signer),/NotRetryEligible/);
      }
    }
    assert.notEqual(attempts[0].nonce,attempts[1].nonce);
    return {id,attempts,phase:'Completed',ethereumCustody:60,ethereumRecipient:40,compactConsumer:60,sealedRetired:40,liveConsensusProofs:true};
  } finally { unsubscribe(); captureSubscriptions.get(dir)?.(); captureSubscriptions.delete(dir); }
}

export function gatewayDispatchLogs(receipt,gateway) {
  const topic=keccakAsHex(Buffer.from('InboundMessageDispatched(uint64,bytes32,bool,bytes32)'));
  return receipt.logs.filter(log=>log.address.toLowerCase()===gateway.toLowerCase() && log.topics[0]?.toLowerCase()===topic);
}
