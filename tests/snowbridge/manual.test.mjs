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
import { mkdtempSync, readFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { checkpoint, atomicJson, observedSubmission } from './manual-control.mjs';
const sleep=ms=>new Promise(resolve=>setTimeout(resolve,ms));
async function respond(dir,response) {
  const path=join(dir,'manual/pending.json');
  while(!existsSync(path)) await sleep(10);
  const request=JSON.parse(readFileSync(path));
  atomicJson(join(dir,'manual',`${request.id}.response.json`),{id:request.id,...response});
}
test('manual gate waits for a matching operator response; abort cannot advance',async()=>{
  for(const action of ['continue','abort']) {
    const dir=mkdtempSync(join(tmpdir(),'bridge-manual-'));
    const result=checkpoint(dir,'inspection',{kind:'stage'},true);
    await respond(dir,{action});
    if(action==='abort') await assert.rejects(result,/aborted/);
    else assert.equal((await result).action,'continue');
  }
});
test('manual gate rejects UI acknowledgements for helpers and stale IDs',async()=>{
  for(const response of [{action:'submitted'},{action:'continue',id:'wrong'}]) {
    const dir=mkdtempSync(join(tmpdir(),'bridge-manual-'));
    const result=checkpoint(dir,'helper',{kind:'helper'},true);
    await respond(dir,response);
    await assert.rejects(result,/only extrinsics|stale manual/);
  }
});
test('observed submission rejects noncanonical blocks before attributing events',async()=>{
  const hash='0x'+'11'.repeat(32),txHash='0x'+'22'.repeat(32);
  const api={rpc:{chain:{getBlock:async()=>({block:{header:{number:3}}}),getHeader:async()=>({number:4}),
    getFinalizedHead:async()=>hash,getBlockHash:async()=>({toHex:()=> '0x'+'33'.repeat(32)})}}};
  await assert.rejects(observedSubmission(api,{},null,{blockHash:hash,extrinsicHash:txHash}),/not canonical/);
});
