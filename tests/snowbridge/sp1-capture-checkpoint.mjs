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
import {readFileSync,writeFileSync} from 'node:fs';
import {join} from 'node:path';
import {keccak256, SigningKey} from 'ethers';
// Recreate the saved single-authority bootstrap from a key anchored against BOTH saved roots.
const dir=process.argv[2], file=process.argv[3], out=process.argv[4];
const anchor=JSON.parse(readFileSync(join(dir,'beefy-checkpoint.json')));
const b=readFileSync(join(dir,file));let i=1;
function compact(){const v=b[i++],tag=v&3;if(tag===0)return v>>2;if(tag===1)return (v+(b[i++]<<8))>>2;throw Error('unsupported fixture length');}
assert.equal(b[0],1);const n=compact(),payload=new Map();
for(let j=0;j<n;j++){const key=b.subarray(i,i+2).toString();i+=2;const len=compact();payload.set(key,b.subarray(i,i+len));i+=len;}
const cs=payload.get('cs'); assert.equal(cs[0],4);assert.equal(cs.length,42);
assert.equal(cs.readBigUInt64LE(34),1n);
const pub=SigningKey.computePublicKey('0x'+cs.subarray(1,34).toString('hex'),false);
const address=Buffer.from(keccak256('0x'+pub.slice(4)).slice(-40),'hex');
for(const key of ['current','next']) {assert.equal(anchor[key].len,1);assert.equal(anchor[key].root,keccak256(address));}
const stakeRoot=Buffer.from(keccak256(Buffer.concat([address,cs.subarray(34)])).slice(2),'hex');
const set=id=>{const s=Buffer.alloc(44);s.writeBigUInt64LE(BigInt(id));s.writeUInt32LE(1,8);stakeRoot.copy(s,12);return s;};
writeFileSync(out,Buffer.concat([Buffer.from(anchor.midnightGenesis.slice(2),'hex'),Buffer.alloc(4),set(anchor.current.id),Buffer.from([1]),set(anchor.next.id)]));
