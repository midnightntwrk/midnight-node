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

import { ApiPromise, WsProvider, Keyring } from '@polkadot/api';
import { readFileSync, writeFileSync } from 'node:fs';
import assert from 'node:assert/strict';
import { join } from 'node:path';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import * as rt from '@midnight-ntwrk/compact-runtime';
import { prepareLiveBridge, redeemLive } from './outbound-live.mjs';
import { beforeSubmission, checkpoint } from './manual-control.mjs';
import * as escrowContract from '../../artifacts/snowbridge/compact/escrow/contract/index.js';

const endpoint = `ws://127.0.0.1:${process.env.SNOWBRIDGE_NODE_PORT ?? 19944}`;
const provider = new WsProvider(endpoint);
const api = await ApiPromise.create({ provider,
  signedExtensions: { AuthorizeCall: { extrinsic: {}, payload: {} }, CheckCallFilter: { extrinsic: {}, payload: {} },
    CheckThrottle: { extrinsic: {}, payload: {} }, WeightReclaim: { extrinsic: {}, payload: {} } } });
const keyring = new Keyring({ type: 'sr25519' });
const keys = Object.fromEntries(['Alice','Bob','Charlie','Dave','Eve','Ferdie'].map(name => [name,keyring.addFromUri(`//${name}`)]));
const dir=process.env.SNOWBRIDGE_PROOF_DIR;
assert(dir,'SNOWBRIDGE_PROOF_DIR is required');
const verifyOnly=process.env.SNOWBRIDGE_VERIFY_ONLY === '1';
const evidence = verifyOnly ? JSON.parse(readFileSync(join(dir,'midnight-beacon-evidence-before-verifier-fix.json'))) : { profile: 'live-pos-native-ingress', completeBridgeE2E: false, nativeTokenIssuedLive: false, calls: [] };
function dispatchError(error) {
  if (error.isModule) {
    const decoded = api.registry.findMetaError(error.asModule);
    return `${decoded.section}.${decoded.name}: ${decoded.docs.join(' ')}`;
  }
  return error.toString();
}
async function send(tx, signer) {
  console.log(`Submitting ${tx.method.section}.${tx.method.method} (${tx.encodedLength} bytes)`);
  const result = await beforeSubmission(api,dir,tx,signer) ?? await new Promise((resolve,reject) => {
    let unsubscribe;
    const timeout = setTimeout(() => { unsubscribe?.(); reject(new Error('transaction finality timeout')); },180_000);
    const callback = result => {
      if (result.isError) { clearTimeout(timeout); unsubscribe?.(); reject(new Error(result.status.toString())); }
      if (!result.status.isFinalized) return;
      clearTimeout(timeout); unsubscribe?.();
      resolve(result);
    };
    const pending=signer ? tx.signAndSend(signer,{nonce:-1},callback) : tx.send(callback);
    pending.then(fn => { unsubscribe=fn; }).catch(error => { clearTimeout(timeout); reject(error); });
  });
  const events = result.events.map(({event}) => ({section:event.section,method:event.method,data:event.data.toJSON()}));
  evidence.calls.push({call:`${tx.method.section}.${tx.method.method}`,tx:result.transactionHash ?? tx.hash.toHex(),block:result.status.asFinalized.toHex(),events});
  writeFileSync(join(dir,'midnight-beacon-evidence.json'),JSON.stringify(evidence,null,2)+'\n');
  if (result.dispatchError) throw new Error(dispatchError(result.dispatchError));
  for (const {event} of result.events) {
    if ((event.method === 'Executed' || event.method === 'MotionDispatched') && event.data.at(-1)?.isErr) {
      throw new Error(`Nested dispatch failed: ${dispatchError(event.data.at(-1).asErr)}`);
    }
  }
  return result;
}
async function root(call) {
  // Exercise the normal two-body governance path using the dev genesis keys.
  const approve = api.tx.federatedAuthority.motionApprove(call.method);
  for (const body of ['council','technicalCommittee']) {
    const members = await api.query[body].members();
    const signers=Object.entries(keys).filter(([,key])=>members.some(m=>m.toHex() === '0x'+Buffer.from(key.publicKey).toString('hex')));
    console.log(body,'members',members.toJSON(),'known signers',signers.map(([name])=>name));
    assert(signers.length >= 2 && members.length <= 3,'expected a development collective with a two-member quorum');
    const [one,two]=signers.map(([name])=>name);
    const result = await send(api.tx[body].propose(2,approve.method,approve.method.encodedLength),keys[one]);
    const event = result.events.find(({event}) => event.section===body && event.method==='Proposed')?.event;
    assert(event, 'missing collective proposal');
    const index=event.data[1],hash=event.data[2];
    for (const name of [one,two]) {
      const voting=(await api.query[body].voting(hash)).unwrap();
      if (!voting.ayes.some(member=>member.toHex()==='0x'+Buffer.from(keys[name].publicKey).toString('hex'))) {
        await send(api.tx[body].vote(hash,index,true),keys[name]);
      }
    }
    const weight={refTime:150_000_000_000,proofSize:350_000};
    await send(api.tx[body].close(hash,index,weight,approve.method.encodedLength),keys[one]);
  }
  const weight={refTime:150_000_000_000,proofSize:350_000};
  return send(api.tx.federatedAuthority.motionClose(call.method.hash,weight),keys.Alice);
}
function decodeArg(method,path,index=0) {
  const type=method.meta.args[index].type.toString();
  const bytes=new Uint8Array(readFileSync(path));
  const value=api.registry.createType(type,bytes);
  assert.deepEqual(value.toU8a(),bytes,`SCALE roundtrip mismatch: ${path}`);
  return value;
}
async function verifyCompact(ledgerDir,before) {
  const manifest=JSON.parse(readFileSync(join(ledgerDir,'compact-manifest.json')));
      const final=(await api.rpc.chain.getFinalizedHead()).toHex();
      for (const [name,balance] of [['consumer',60n],['escrow',40n]]) {
        const encoded=await provider.send('midnight_contractState',[manifest[name],final]);
        const bytes=Buffer.from(encoded.replace(/^0x/,''),'hex');
        writeFileSync(join(ledgerDir,`live-${name}-state.bin`),bytes);
        const state=rt.ContractState.deserialize(bytes);
        assert.deepEqual([...state.balance],[[{tag:'unshielded',raw:manifest.color},balance]]);
        const expected=rt.ContractState.deserialize(readFileSync(join(ledgerDir,`expected-${name}-state.bin`)));
        assert.deepEqual(state.data.state.encode(),expected.data.state.encode());
        if (name==='escrow') {
          const record=escrowContract.ledger(state.data);
          assert.equal(record.nextSequence,1n);
          assert.equal(record.totalRetired,40n);
          assert.equal(record.records.size(),1n);
          assert.deepEqual(record.records.lookup(0n),{amount:40n,recipient:new Uint8Array(20).fill(5)});
          assert.deepEqual(state.operations(),['retire']);
          assert.deepEqual(state.maintenanceAuthority.committee,[]);
          assert.equal(state.maintenanceAuthority.threshold,1);
        }
      }
      const stats=await provider.send('midnight_ledgerStats',[final]);
      assert.equal(stats.contractCount,2);
      assert.equal(stats.unshieldedUtxoCount,before.unshieldedUtxoCount);
      assert.equal(stats.unshieldedUtxoStars,before.unshieldedUtxoStars);
      assert(stats.dustNullifierCount>=before.dustNullifierCount+6);
      evidence.liveCompactConsumerRoundTrip=true;
      evidence.compact={consumerBalance:60,escrowBalance:40,walletBalance:0,retirementSequence:0,recipient:'05'.repeat(20),finalizedHash:final,ledgerStats:stats};

}

try {
  assert(api.tx.ethereumBeaconClient && api.tx.ethereumBridge, 'wrong runtime');
  await checkpoint(dir,'Local chains ready: connect Polkadot.js and the read-only state viewer',{kind:'stage',endpoint});
  if (verifyOnly) {
    assert.equal(evidence.failure,'TypeError: state.data.encode is not a function','readback is only for the known post-transaction verifier error');
    assert(evidence.nativeTokenIssuedLive && evidence.reissuanceAfterSpendingRejected);
    assert.equal(evidence.compactSteps.length,5);
    const finalHeader=await api.rpc.chain.getHeader(await api.rpc.chain.getFinalizedHead());
    const calls=evidence.calls.filter(call=>call.call==='midnight.sendMnTransaction').slice(-5);
    assert.equal(calls.length,5);
    for (const call of calls) {
      const header=await api.rpc.chain.getHeader(call.block);
      assert(BigInt(finalHeader.number.toString())>=BigInt(header.number.toString()));
      assert.equal((await api.rpc.chain.getBlockHash(header.number)).toHex(),call.block);
      const expected=call.events.find(event=>event.section==='midnight' && event.method==='TxApplied');
      assert(expected);
      const events=await (await api.at(call.block)).query.system.events();
      assert(events.some(({event})=>event.section===expected.section && event.method===expected.method && JSON.stringify(event.data.toJSON())===JSON.stringify(expected.data)),'recorded application is not in canonical finalized history');
    }
    await verifyCompact(join(dir,'ledger'),evidence.ledgerBefore);
    evidence.verifierRepair={priorFailure:evidence.failure,persistedFinalizedHistoryRechecked:true};
    delete evidence.failure;
  } else {
  if (process.env.SNOWBRIDGE_LIVE_OUTBOUND === '1') await prepareLiveBridge(api,provider,dir);
  await root(api.tx.ethereumBeaconClient.forceCheckpoint(decodeArg(api.tx.ethereumBeaconClient.forceCheckpoint,join(dir,'bootstrap/checkpoint.scale'))));
  evidence.bootstrapFinalized=true;
  const updateDir=process.env.SNOWBRIDGE_RECEIPT_DIR;
  if (updateDir) {
    await send(api.tx.ethereumBeaconClient.submit(decodeArg(api.tx.ethereumBeaconClient.submit,join(updateDir,'update.scale'))),keys.Alice);
    evidence.liveBeaconUpdateVerified=true;
  }
  if (process.env.SNOWBRIDGE_LIVE_ISSUANCE === '1') {
    const ledgerDir=join(dir,'ledger');
    const config=JSON.parse(readFileSync(join(ledgerDir,'registry.json')));
    const before=await provider.send('midnight_ledgerStats',[(await api.rpc.chain.getFinalizedHead()).toHex()]);
    assert.equal(before.unshieldedUtxoCount,Number(process.env.SNOWBRIDGE_FEE_OUTPUTS ?? 1));
    assert.equal(before.contractCount,0);
    evidence.ledgerBefore=before;
    const deployment=await send(api.tx.midnight.sendMnTransaction('0x'+readFileSync(join(ledgerDir,'escrow-proven.bin')).toString('hex')));
    assert(deployment.events.some(({event})=>event.section==='midnight' && event.method==='ContractDeploy'));
    await root(api.tx.ethereumBridge.activate(config));
    const event=decodeArg(api.tx.ethereumBridge.submitDeposit,join(updateDir,'event.scale'));
    const accepted=await send(api.tx.ethereumBridge.submitDeposit(event),keys.Alice);
    const id=accepted.events.find(({event})=>event.section==='ethereumBridge' && event.method==='Pending')?.event.data[0];
    assert(id,'deposit was not admitted');
    const pending=(await api.query.ethereumBridge.deposits(id)).unwrap();
    assert.equal(pending.credit.amount.toString(),'100');
    assert.equal(pending.credit.color.toHex(),config.escrow.color);
    assert.equal(pending.issued.isTrue,false);
    const issued=await send(api.tx.ethereumBridge.materializeDeposit(id),keys.Alice);
    const systemEvent=issued.events.find(({event})=>event.section==='midnightSystem' && event.method==='SystemTransactionApplied')?.event;
    assert(systemEvent,'missing canonical ledger issuance event');
    const serialized=systemEvent.data[0].serializedSystemTransaction.toU8a(true);
    writeFileSync(join(ledgerDir,'live-issuance.bin'),serialized);
    evidence.issuanceTimestampMs=(await (await api.at(issued.status.asFinalized)).query.timestamp.now()).toString();
    assert.equal((await api.query.ethereumBridge.deposits(id)).unwrap().issued.isTrue,true);
    await assert.rejects(()=>send(api.tx.ethereumBridge.materializeDeposit(id),keys.Alice),/AlreadyIssued/);
    await assert.rejects(()=>send(api.tx.ethereumBridge.submitDeposit(event),keys.Alice),/AlreadySeen/);
    const at=(await api.rpc.chain.getFinalizedHead()).toHex();
    const after=await provider.send('midnight_ledgerStats',[at]);
    assert.equal(after.unshieldedUtxoCount,before.unshieldedUtxoCount+1);
    assert.equal(after.contractCount,1);
    assert.equal(after.unshieldedUtxoStars,before.unshieldedUtxoStars);
    assert(after.dustNullifierCount>before.dustNullifierCount,'deployment did not spend DUST');
    evidence.ledgerAfter=after;
    evidence.ledgerRoot=await provider.send('midnight_ledgerStateRoot',[at]);
    evidence.nativeTokenIssuedLive=true;
    evidence.credit=pending.credit.toJSON();
    evidence.depositId=id.toHex();
    evidence.duplicateIssuanceRejected=true;
    evidence.duplicateReceiptRejected=true;
    if (process.env.SNOWBRIDGE_LIVE_COMPACT === '1') {
      await checkpoint(dir,'Native issuance finalized: generate proofs for the five Compact transactions',{kind:'stage'});
      const log=join(dir,'compact-prove.log');
      try {
        const result=await promisify(execFile)('cargo',['test','--locked','--manifest-path','tests/snowbridge/rust/Cargo.toml',
          '--features','compact-ledger','prove_live_compact_round_trip','--','--ignored','--nocapture'],
          {env:{...process.env,SNOWBRIDGE_LIVE_LEDGER_DIR:ledgerDir},timeout:1_800_000,maxBuffer:16*1024*1024});
        writeFileSync(log,result.stdout+result.stderr);
      } catch (error) {
        writeFileSync(log,(error.stdout??'')+(error.stderr??'')+String(error));
        throw error;
      }
      const manifest=JSON.parse(readFileSync(join(ledgerDir,'compact-manifest.json')));
      evidence.compactSteps=[];
      for (let index=0;index<5;index++) {
        await checkpoint(dir,`Compact ${index}: ${['deploy consumer','deposit 100','query balance 100','withdraw 40','retire 40'][index]}`,{kind:'stage',consumer:manifest.consumer,escrow:manifest.escrow,color:manifest.color});
        const result=await send(api.tx.midnight.sendMnTransaction('0x'+readFileSync(join(ledgerDir,`compact-proven-${index}.bin`)).toString('hex')));
        assert(result.events.some(({event})=>event.section==='midnight' && event.method==='TxApplied'),'Compact transaction did not fully apply');
        assert(!result.events.some(({event})=>event.method==='TxPartialSuccess'));
        const at=result.status.asFinalized.toHex();
        const stats=await provider.send('midnight_ledgerStats',[at]);
        const expectedUtxos=before.unshieldedUtxoCount+([1,0,0,1,0][index]);
        assert.equal(stats.unshieldedUtxoCount,expectedUtxos);
        const encoded=await provider.send('midnight_contractState',[manifest.consumer,at]);
        const state=rt.ContractState.deserialize(Buffer.from(encoded.replace(/^0x/,''),'hex'));
        const balance=[...state.balance].find(([token])=>token.tag==='unshielded' && token.raw===manifest.color)?.[1] ?? 0n;
        assert.equal(balance,[0n,100n,100n,60n,60n][index]);
        evidence.compactSteps.push({index,finalizedHash:at,consumerBalance:balance.toString(),unshieldedUtxoCount:stats.unshieldedUtxoCount});
      }
      await assert.rejects(()=>send(api.tx.ethereumBridge.materializeDeposit(id),keys.Alice),/AlreadyIssued/);
      evidence.reissuanceAfterSpendingRejected=true;
      await verifyCompact(ledgerDir,before);
      if (process.env.SNOWBRIDGE_LIVE_OUTBOUND === '1') {
        evidence.outbound = await redeemLive({api,provider,dir,send,decodeArg,signer:keys.Alice});
        evidence.outboundRedemptionLive = true;
        evidence.completeBridgeE2E = true;
      }
    }

  }
  }
  evidence.genesisHash=api.genesisHash.toHex();
  evidence.finalizedHash=(await api.rpc.chain.getFinalizedHead()).toHex();
  writeFileSync(join(dir,'midnight-beacon-evidence.json'),JSON.stringify(evidence,null,2)+'\n');
  console.log('PASS: live Ethereum finality verified in Midnight'+(evidence.nativeTokenIssuedLive?' and 100 native tokens issued exactly once.':'.')+(evidence.liveCompactConsumerRoundTrip?' Compact balances verified: consumer 60, escrow 40.':'')+(evidence.outboundRedemptionLive?' Ethereum redemption completed after authenticated failure and retry: custody 60, recipient 40.':''));
  await checkpoint(dir,'Flow assertions passed: inspect final states; continue to audit and shut down',{kind:'stage'});
} catch (error) {
  evidence.failure=String(error);
  throw error;
} finally {
  writeFileSync(join(dir,'midnight-beacon-evidence.json'),JSON.stringify(evidence,null,2)+'\n');
  await api.disconnect();
}
