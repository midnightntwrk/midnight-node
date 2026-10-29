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

import {readFileSync, writeFileSync, mkdirSync} from 'node:fs';
import * as rt from '@midnight-ntwrk/compact-runtime';
import * as led from '@midnightntwrk/ledger-v9';
import * as escrow from '../../artifacts/snowbridge/compact/escrow/contract/index.js';
import * as consumer from '../../artifacts/snowbridge/compact/consumer/contract/index.js';

const artifacts = new URL('../../artifacts/snowbridge/', import.meta.url);
const output = new URL('compact-ledger/', artifacts);
mkdirSync(output, {recursive:true});
const coinKey='01'.repeat(32);
const ttl=new Date(100000);
const owner=led.addressFromKey(led.signatureVerifyingKey(led.signingKeyFromBip340(new Uint8Array(32).fill(1))));
const namespace=new Uint8Array(32).fill(3);
async function deploy(name, module, args) {
  const contract=new module.Contract({});
  const initial=await contract.initialState(rt.createConstructorContext({},coinKey),...args);
  const state=initial.currentContractState;
  for (const op of state.operations()) {
    const operation=state.operation(op);
    operation.verifierKey=readFileSync(new URL(`compact/${name}/keys/${op}.verifier`,artifacts));
    state.setOperation(op,operation);
  }
  const ledgerState=led.ContractState.deserialize(state.serialize());
  const deployment=new led.ContractDeploy(ledgerState);
  writeFileSync(new URL(`${name}-deploy.bin`,output),led.Intent.new(ttl).addDeploy(deployment).serialize());
  return {name,contract,state,address:deployment.address};
}
const retirement=await deploy('escrow',escrow,[new Uint8Array(32).fill(2),namespace]);
const color=rt.rawTokenType(namespace,retirement.address);
const application=await deploy('consumer',consumer,[Buffer.from(color,'hex')]);
async function call(target,operation,args,balance=0n) {
  target.state.balance=new Map([[{tag:'unshielded',raw:color},balance]]);
  const ctx=rt.createCircuitContext(operation,target.address,coinKey,target.state,{},undefined,undefined,undefined,2);
  const result=await target.contract.circuits[operation](ctx,...args);
  const trace=result.context.callProofDataTrace[0];
  const ledgerCtx=new led.QueryContext(new led.ChargedState(led.StateValue.decode(trace.initialQueryContext.state.state.encode())),target.address);
  ledgerCtx.block=trace.initialQueryContext.block;
  ledgerCtx.effects=trace.initialQueryContext.effects;
  const [guaranteed,fallible]=led.partitionTranscripts([new led.PreTranscript(ledgerCtx,trace.publicTranscript)],led.LedgerParameters.initialParameters())[0];
  const prototype=new led.ContractCallPrototype(target.address,operation,
    led.ContractOperation.deserialize(target.state.operation(operation).serialize()),
    guaranteed,fallible,trace.privateTranscriptOutputs,trace.input,trace.output,
    led.communicationCommitmentRandomness(),operation);
  const intent=led.Intent.new(ttl).addCall(prototype);
  writeFileSync(new URL(`${target.name}-${operation}.bin`,output),intent.serialize());
  target.state.data=result.context.callContext.currentQueryContext.state;
}
await call(application,'deposit',[100n]);
await call(application,'balance',[],100n);
await call(application,'withdraw',[40n,Buffer.from(owner,'hex')],100n);
await call(retirement,'retire',[40n,new Uint8Array(20).fill(5)]);
const manifest={owner,color,escrow:retirement.address,consumer:application.address};
writeFileSync(new URL('manifest.json',output),JSON.stringify(manifest,null,2)+'\n');
console.log(JSON.stringify(manifest));
