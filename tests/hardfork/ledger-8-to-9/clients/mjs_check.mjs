#!/usr/bin/env node
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

// Midnight.js checks across the fork. Midnight.js 5.x dispatches on the ledger era, so a
// contract deployed before the fork stays callable with the artefacts it was compiled with.
//
//   mjs_check.mjs head                                    era of the indexer's head
//   mjs_check.mjs decode-state --address <a>              the decoded state a dApp reads
//   mjs_check.mjs state-data --hex-file <f>               hash of a serialised state's data
//   mjs_check.mjs events --address <a> [--expect n]       contract events from the indexer
//   mjs_check.mjs deploy --contract-dir <d> --name <n>
//   mjs_check.mjs call --contract-dir <d> --name <n> --address <a> [--circuit c] [--times n]
//   mjs_check.mjs retained-deploy | retained-call         the same with ledger-8 artefacts
//
// --contract-dir is a compactc output directory: compactc 0.31.x for the retained
// (ledger-8) era, the compiler matching this workspace's compact-runtime for the current one.

import { createHash } from 'node:crypto';
import fs from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';

import { deployContract, findDeployedContract, submitCallTx } from '@midnight-ntwrk/midnight-js-contracts';
import { CompiledContract } from '@midnight-ntwrk/midnight-js-protocol/compact-js';
import { setNetworkId } from '@midnight-ntwrk/midnight-js-network-id';

import { config, emit, flag, intFlag, log, runCommands, txTimeoutSecs, UsageError, withTimeout } from './lib/config.mjs';
import { resolvedForks, withWallet } from './lib/wallet.mjs';
import { buildProviders, publicDataProvider } from './lib/mjs_providers.mjs';

const require = createRequire(import.meta.url);

// The witnesses and private state of the toolkit-js configs in dapps/config/.
const DAPPS = {
  counter: {
    witnesses: { private_increment: ({ privateState }) => [{ count: (privateState?.count ?? 0) + 1 }, []] },
    initialPrivateState: { count: 0 },
  },
  bboard: {
    witnesses: { local_secret_key: ({ privateState }) => [privateState, Uint8Array.from(Buffer.from(privateState.secretKey, 'hex'))] },
    initialPrivateState: { secretKey: '01'.repeat(32) },
  },
};

const required = (...names) => {
  const missing = names.filter((n) => !flag(n));
  if (missing.length) log(`missing --${missing.join(', --')}`);
  return missing.length === 0;
};

// The target contract from the flags. The current era takes a CompiledContract; the
// retained era takes the raw instance and dispatches on the runtime its artefact declares.
const target = (retained) => {
  const name = flag('name', 'counter');
  const contractDir = path.resolve(flag('contract-dir'));
  const { witnesses = {}, initialPrivateState } = DAPPS[name] ?? {};
  const { Contract } = require(path.join(contractDir, 'contract', 'index.js'));
  let compiledContract;
  if (retained) {
    compiledContract = new Contract(witnesses);
  } else {
    compiledContract = CompiledContract.make(name, Contract);
    if (Object.keys(witnesses).length > 0) compiledContract = CompiledContract.withWitnesses(compiledContract, witnesses);
    compiledContract = CompiledContract.withCompiledFileAssets(compiledContract, contractDir);
  }
  return { name, contractDir, compiledContract, initialPrivateState, privateStateId: retained ? `${name}-retained` : name };
};

const withProviders = (t, fn) => {
  setNetworkId(config.networkId);
  const forks = resolvedForks();
  return withWallet(({ facade, state }) =>
    fn(buildProviders({ facade, state, forks, contractDir: t.contractDir, privateStateId: t.privateStateId })),
  );
};

const deploy = (retained) => async () => {
  if (!required('contract-dir')) return 2;
  const t = target(retained);
  const psId = retained ? `${t.name}-retained-deploy` : t.name;
  return withProviders(t, async (providers) => {
    const deployed = await inTime(deployContract(providers, {
      compiledContract: t.compiledContract,
      ...(t.initialPrivateState === undefined ? {} : { privateStateId: psId, initialPrivateState: t.initialPrivateState }),
    }), 'deploy');
    const pub = deployed.deployTxData.public;
    emit({ deployed: true, name: t.name, contractAddress: pub.contractAddress, txId: pub.txId, blockHeight: pub.blockHeight });
  });
};

// Proving, submission and Midnight.js's watchForTxData, which has no timeout of its own.
const inTime = (promise, what) => withTimeout(promise, txTimeoutSecs(), `${what} (MN_TX_TIMEOUT_SECS; it may still land)`);

// Ledger 8 serialises contract-state[v6], ledger 9 contract-state[v8]: the operations and the
// authority are re-encoded at the fork, the data must not change.
const LEDGERS = { 'contract-state[v6]': '@midnight-ntwrk/ledger-v8', 'contract-state[v8]': '@midnightntwrk/ledger-v9' };
const canonical = (v) => JSON.stringify(v, (_k, x) => {
  if (x instanceof Uint8Array) return Buffer.from(x).toString('hex');
  if (typeof x === 'bigint') return x.toString();
  return x instanceof Map ? [...x.entries()] : x;
});

const repeat = async (times, fn) => {
  const results = [];
  for (let i = 0; i < times; i += 1) results.push(await fn(i));
  return results;
};

runCommands({
  async head() {
    const version = await publicDataProvider().queryLatestProtocolVersion();
    const forks = resolvedForks();
    emit({ headProtocolVersion: version, forks, era: Number(version) >= Number(forks.v9) ? 'v9' : 'v8' });
  },

  // The provider picks the decoder from the era the indexer labels the bytes with, so this
  // fails when the indexer serves bytes of the other era.
  async 'decode-state'() {
    if (!required('address')) return 2;
    const address = flag('address');
    const provider = publicDataProvider();
    let base = { address };
    try {
      const versioned = await provider.queryRawContractState(address);
      const raw = typeof versioned?.raw === 'string' ? Buffer.from(versioned.raw, 'hex') : versioned?.raw;
      const tag = raw instanceof Uint8Array ? (new TextDecoder().decode(raw.slice(0, 40)).match(/midnight:contract-state\[v\d+\]/)?.[0] ?? null) : null;
      base = { address, labelledEra: versioned?.version ?? null, protocolVersion: versioned?.protocolVersion ?? null, tag };
      const state = await provider.queryContractState(address);
      emit({ ...base, decoded: Boolean(state) });
      return state ? 0 : 1;
    } catch (err) {
      log(`FAIL: ${err?.name ?? 'Error'}: ${String(err?.message ?? err).slice(0, 220)}`);
      emit({ ...base, decoded: false, error: err?.name ?? 'Error' });
      return 1;
    }
  },

  'state-data'() {
    if (!required('hex-file')) return 2;
    const raw = Buffer.from(fs.readFileSync(flag('hex-file'), 'utf8').trim().replace(/^0x/, ''), 'hex');
    const tag = raw.subarray(0, 48).toString('latin1').match(/contract-state\[v\d+\]/)?.[0];
    if (!LEDGERS[tag]) throw new Error(`no decoder for '${tag ?? 'no tag'}'`);
    const state = require(LEDGERS[tag]).ContractState.deserialize(raw);
    const dataHash = createHash('sha256').update(canonical(state.data.state.encode())).digest('hex');
    emit({ tag, dataHash, operations: state.operations().length });
  },

  async events() {
    if (!required('address')) return 2;
    const address = flag('address');
    const expect = intFlag('expect', undefined, 0);
    const events = await publicDataProvider().queryContractEvents({ contractAddress: address }, { limit: 100 });
    const summary = events.map((ev) => ({
      eventType: ev.eventType,
      version: ev.version,
      contractAddress: ev.contractAddress,
      ...(ev.name !== undefined && { name: ev.name }),
      ...(ev.nullifier !== undefined && { nullifier: ev.nullifier }),
      ...(ev.payload !== undefined && { payloadBytes: String(ev.payload).replace(/^0x/, '').length / 2 }),
    }));
    emit({ address, via: 'queryContractEvents', count: events.length, events: summary });
    if (expect !== undefined && expect !== events.length) {
      log(`FAIL: expected ${expect} events, the provider returned ${events.length}`);
      return 1;
    }
    return 0;
  },

  deploy: deploy(false),
  'retained-deploy': deploy(true),

  async call() {
    if (!required('contract-dir', 'address')) return 2;
    const times = intFlag('times', 1, 1);
    const t = target(false);
    const address = flag('address');
    const circuit = flag('circuit', 'increment');
    return withProviders(t, async (providers) => {
      const found = await findDeployedContract(providers, {
        compiledContract: t.compiledContract,
        contractAddress: address,
        privateStateId: t.name,
        initialPrivateState: t.initialPrivateState,
      });
      const results = await repeat(times, async () => {
        if (!found.callTx[circuit]) throw new UsageError(`${t.name} has no circuit '${circuit}'`);
        const { public: pub = {} } = await inTime(found.callTx[circuit](), `${circuit}`);
        return { txId: pub.txId, blockHeight: pub.blockHeight, status: pub.status };
      });
      emit({ called: true, name: t.name, address, circuit, times, results });
    });
  },

  // A pre-fork contract called after the fork with its ledger-8 artefacts, several times:
  // one that could be called only once would be a regression.
  async 'retained-call'() {
    if (!required('contract-dir', 'address')) return 2;
    const times = intFlag('times', 3, 1);
    const t = target(true);
    const address = flag('address');
    const circuit = flag('circuit', 'increment');
    const psId = `${t.name}-retained-${address.slice(0, 8)}`;
    return withProviders(t, async (providers) => {
      providers.privateStateProvider.setContractAddress?.(address);
      if ((await providers.privateStateProvider.get(psId)) == null) {
        await providers.privateStateProvider.set(psId, t.initialPrivateState ?? {});
      }
      const results = await repeat(times, async (i) => {
        const { public: pub = {} } = await inTime(submitCallTx(providers, {
          compiledContract: t.compiledContract,
          contractAddress: address,
          circuitId: circuit,
          privateStateId: psId,
        }), `${circuit}`);
        log(`  ${circuit} ${i + 1}/${times}: ${pub.status} at block ${pub.blockHeight}, era ${pub.version}`);
        return { txId: pub.txId, blockHeight: pub.blockHeight, status: pub.status, version: pub.version };
      });
      emit({ retainedCall: true, name: t.name, address, circuit, times, results });
    });
  },
});
