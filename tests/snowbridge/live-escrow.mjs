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

import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { join } from 'node:path';
import { keccakAsU8a } from '@polkadot/util-crypto';
import * as rt from '@midnight-ntwrk/compact-runtime';
import * as led from '@midnightntwrk/ledger-v9';
import * as escrow from '../../artifacts/snowbridge/compact/escrow/contract/index.js';

const run=process.env.SNOWBRIDGE_PROOF_DIR;
if (!run) throw new Error('SNOWBRIDGE_PROOF_DIR required');
const output=join(run,'ledger');
mkdirSync(output,{recursive:true});
const deployment=JSON.parse(readFileSync(join(run,'deployment.json')));
const bytes=x=>Buffer.from(x.replace(/^0x/,''),'hex');
const hex=x=>'0x'+Buffer.from(x).toString('hex');
const word=text=>{const result=Buffer.alloc(32);result.write(text);return result;};
const domain=bytes(deployment.bridgeDomain);
const assetId=keccakAsU8a(Buffer.concat([word('MN/SB/ASSET/V1'),domain,Buffer.concat([Buffer.alloc(12),bytes(deployment.token)])]));
const namespace=keccakAsU8a(Buffer.concat([word('MN/SB/COLOR/V1'),assetId]));
const contract=new escrow.Contract({});
const initial=await contract.initialState(rt.createConstructorContext({},'01'.repeat(32)),domain,namespace);
const state=initial.currentContractState;
const operation=state.operation('retire');
operation.verifierKey=readFileSync(new URL('../../artifacts/snowbridge/compact/escrow/keys/retire.verifier',import.meta.url));
state.setOperation('retire',operation);
writeFileSync(join(output,'escrow-initial-state.bin'),state.serialize());
const deploy=new led.ContractDeploy(led.ContractState.deserialize(state.serialize()));
const intent=led.Intent.new(new Date(Date.now()+3_600_000)).addDeploy(deploy);
writeFileSync(join(output,'escrow-deploy.bin'),intent.serialize());
const config={gateway:deployment.gateway,token:deployment.token,
  escrow:{bridgeDomain:hex(domain),tokenDomain:hex(namespace),escrow:'0x'+deploy.address,
    color:'0x'+rt.rawTokenType(namespace,deploy.address)}};
writeFileSync(join(output,'registry.json'),JSON.stringify(config,null,2)+'\n');
console.log('Prepared live Compact escrow',deploy.address);
