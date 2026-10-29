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
import { existsSync, readFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { createServer } from 'node:http';
import { atomicJson } from './manual-control.mjs';

const [action, directory, ...args] = process.argv.slice(2);
assert(['status','next','submitted','abort','serve'].includes(action) && directory,
  'Usage: node tests/snowbridge/manual.mjs status|next|submitted|abort|serve RUN [STEP_ID] [BLOCK_HASH TX_HASH]');
const dir = resolve(directory), folder = join(dir,'manual');
const read = name => existsSync(join(dir,name)) ? JSON.parse(readFileSync(join(dir,name))) : null;
const pending = () => read('manual/pending.json');
if (action === 'status') {
  console.log(JSON.stringify(pending() ?? {state:'Preparing; no manual checkpoint yet'},null,2));
} else if (action !== 'serve') {
  const request = pending();
  assert(request && !request.resolved, 'No pending step');
  assert.equal(args[0],request.id,'Use the current step ID from status; refusing a stale command');
  const response = {id:request.id,action:action === 'next' ? 'continue' : action};
  if (action === 'submitted') {
    assert.equal(request.kind,'extrinsic');
    for (const hash of args.slice(1)) assert.match(hash,/^0x[0-9a-fA-F]{64}$/);
    assert.equal(args.length,3,'submitted requires STEP_ID BLOCK_HASH TX_HASH');
    response.blockHash=args[1]; response.extrinsicHash=args[2];
  }
  const path=join(folder,`${request.id}.response.json`);
  assert(!existsSync(path),'This step already has a response');
  atomicJson(path,response);
  console.log(`Recorded ${response.action} for ${request.title}`);
} else {
  const {ApiPromise,WsProvider}=await import('@polkadot/api');
  const rt=await import('@midnight-ntwrk/compact-runtime');
  const escrow=await import('../../artifacts/snowbridge/compact/escrow/contract/index.js');
  const provider=new WsProvider(`ws://127.0.0.1:${process.env.SNOWBRIDGE_NODE_PORT ?? 19944}`);
  const api=await ApiPromise.create({provider});
  const port=Number(process.env.SNOWBRIDGE_VIEW_PORT ?? 18080);
  async function eth(method,params=[]) {
    const network=read('network.json');
    const response=await fetch(network.execution,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({jsonrpc:'2.0',id:1,method,params}),signal:AbortSignal.timeout(10000)});
    const result=await response.json();
    assert(!result.error,JSON.stringify(result.error));
    return result.result;
  }
  async function snapshot() {
    const hash=(await api.rpc.chain.getFinalizedHead()).toHex();
    const at=await api.at(hash);
    const header=await api.rpc.chain.getHeader(hash);
    const entries=async query=>(await query.entries()).map(([key,value])=>({key:key.args.map(arg=>arg.toJSON()),value:value.toJSON()}));
    const bridge={registry:(await at.query.ethereumBridge.activeRegistry()).toJSON(),
      deposits:await entries(at.query.ethereumBridge.deposits),redemptions:await entries(at.query.ethereumBridge.redemptions),
      nonces:await entries(at.query.ethereumBridge.byNonce),halted:(await at.query.ethereumBridge.outboundHalted()).toJSON()};
    const contracts={};
    const manifest=read('ledger/compact-manifest.json');
    const registry=read('ledger/registry.json');
    for(const [name,address] of [['consumer',manifest?.consumer],['escrow',registry?.escrow.escrow]]) {
      if(!address) continue;
      try {
        const encoded=await provider.send('midnight_contractState',[address.replace(/^0x/,''),hash]);
        const state=rt.ContractState.deserialize(Buffer.from(encoded.replace(/^0x/,''),'hex'));
        contracts[name]={address,balances:[...state.balance].map(([token,amount])=>({token,amount:String(amount)})),operations:state.operations()};
        if(name==='escrow') {
          const ledger=escrow.ledger(state.data);
          contracts[name].nextSequence=String(ledger.nextSequence);
          contracts[name].totalRetired=String(ledger.totalRetired);
          contracts[name].records=Array.from({length:Number(ledger.nextSequence)},(_,i)=>{
            const record=ledger.records.lookup(BigInt(i));
            return {sequence:i,amount:String(record.amount),recipient:'0x'+Buffer.from(record.recipient).toString('hex')};
          });
        }
      } catch(error) { contracts[name]={address,unavailable:String(error)}; }
    }
    const ethereum={deployment:read('deployment.json')};
    try {
    for(const tag of ['latest','finalized']) {
      const block=await eth('eth_getBlockByNumber',[tag,false]);
      if(!block) {ethereum[tag]={waiting:'No finalized execution block yet'};continue;}
      ethereum[tag]={block:Number(BigInt(block.number)),hash:block.hash};
      if(ethereum.deployment) {
        for(const [name,address] of [['custody',ethereum.deployment.custody],['recipient','0x'+'05'.repeat(20)]]) {
          const value=await eth('eth_call',[{to:ethereum.deployment.token,data:'0x70a08231'+address.replace(/^0x/,'').padStart(64,'0')},block.number]);
          ethereum[tag][name]=value==='0x' ? 'contract not deployed at this block' : String(BigInt(value));
        }
      }
    }
    ethereum.releases=[];
    for(const index of [0,1]) {
      const broadcast=read(`release-broadcast-${index}.json`);
      if(!broadcast) continue;
      const receipt=await eth('eth_getTransactionReceipt',[broadcast.transactions.at(-1).hash]);
      ethereum.releases.push({attempt:index,hash:receipt.transactionHash,block:Number(BigInt(receipt.blockNumber)),transactionStatus:receipt.status,logs:receipt.logs});
    }
    } catch(error) {ethereum.error=String(error);}
    return {observedAt:new Date().toISOString(),pending:pending(),midnight:{finalizedBlock:header.number.toString(),hash,bridge,contracts,
      ledgerStats:await provider.send('midnight_ledgerStats',[hash]),latestBeaconRoot:(await at.query.ethereumBeaconClient.latestFinalizedBlockRoot()).toHex()},
      ethereum,recentCalls:read('midnight-beacon-evidence.json')?.calls?.slice(-8) ?? []};
  }
  let cached={loading:true},busy=false;
  async function refresh() {
    if(busy) return;
    busy=true;
    try {cached=await snapshot();} catch(error) {cached={...cached,stale:true,error:String(error)};} finally {busy=false;}
  }
  await refresh();
  const interval=setInterval(refresh,3000);
  const server=createServer((request,response)=>{
    if(request.method!=='GET') {response.writeHead(405);response.end();return;}
    response.setHeader('Cache-Control','no-store');
    if(request.url==='/state') {response.setHeader('Content-Type','application/json');response.end(JSON.stringify(cached));return;}
    if(request.url!=='/') {response.writeHead(404);response.end();return;}
    response.setHeader('Content-Type','text/html; charset=utf-8');
    response.end(`<!doctype html><html><head><title>Local bridge walkthrough</title><style>
      body{font:16px system-ui;background:#101622;color:#e8eff9;margin:28px}h1{margin-bottom:8px}p{color:#acbbd0}
      main{display:grid;grid-template-columns:repeat(auto-fit,minmax(420px,1fr));gap:18px}section{background:#1b2638;border-radius:10px;padding:18px;min-width:0}pre{white-space:pre-wrap;overflow-wrap:anywhere;font:13px/1.5 monospace}a{color:#8dcbff}#status{padding:12px;background:#263951}.error{color:#ffb4a8}
      </style></head><body><h1>Local bridge walkthrough</h1><p>Read-only. Midnight values use one finalized block. Ethereum shows latest and finalized separately. Refreshes every 3 seconds.</p>
      <p><a href="http://127.0.0.1:18081/?rpc=ws%3A%2F%2F127.0.0.1%3A${process.env.SNOWBRIDGE_NODE_PORT ?? 19944}#/explorer" target="_blank">Local Polkadot.js (if started on 18081)</a></p><p id="status">Connecting…</p><main>
      <section><h2>Next manual step</h2><pre id="pending"></pre></section><section><h2>Midnight bridge</h2><pre id="bridge"></pre></section>
      <section><h2>Compact contracts</h2><pre id="contracts"></pre></section><section><h2>Ethereum custody / recipient</h2><pre id="ethereum"></pre></section>
      <section><h2>Recent finalized calls</h2><pre id="calls"></pre></section><section><h2>Ledger and beacon</h2><pre id="ledger"></pre></section></main>
      <script>async function update(){try{const s=await(await fetch('/state')).json();const text=(id,v)=>document.getElementById(id).textContent=JSON.stringify(v,null,2);document.getElementById('status').textContent=s.error?'STALE: '+s.error:'Observed '+s.observedAt+' — Midnight finalized block '+s.midnight?.finalizedBlock;document.getElementById('status').className=s.error?'error':'';text('pending',s.pending);text('bridge',s.midnight?.bridge);text('contracts',s.midnight?.contracts);text('ethereum',s.ethereum);text('calls',s.recentCalls);text('ledger',{stats:s.midnight?.ledgerStats,beaconRoot:s.midnight?.latestBeaconRoot});}catch(e){document.getElementById('status').textContent='Disconnected: '+e;}}update();setInterval(update,3000);</script></body></html>`);
  });
  server.listen(port,'127.0.0.1',()=>console.log(`Read-only viewer: http://127.0.0.1:${port}`));
  for(const signal of ['SIGINT','SIGTERM']) process.on(signal,async()=>{clearInterval(interval);server.close();await api.disconnect();process.exit(0);});
}
