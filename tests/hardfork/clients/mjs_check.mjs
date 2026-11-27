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

// Midnight.js checks across the fork. Midnight.js dispatches on the ledger era, so a
// contract deployed before the fork stays callable with the artefacts it was compiled with.
//
//   mjs_check.mjs head                                    era of the indexer's head
//   mjs_check.mjs decode-state --address <a> [--height h] the decoded state a dApp reads
//   mjs_check.mjs balances --address <a> [--height h]     the contract's unshielded balances
//   mjs_check.mjs tx-data --tx-id <id>                    a finalized transaction, decoded for its era
//   mjs_check.mjs state-data --hex-file <f>               hash of a serialised state's data
//   mjs_check.mjs events --address <a> [--expect n]       contract events from the indexer
//   mjs_check.mjs deploy --contract-dir <d> --name <n>
//   mjs_check.mjs call --contract-dir <d> --name <n> --address <a> [--circuit c] [--times n]
//                      [--args <json array>] [--expect-error <text>]
//   mjs_check.mjs retained-deploy | retained-call         the same with old-ledger artefacts
//
// --contract-dir is a compactc output directory, from the compiler that targets the
// compact-runtime of the side the contract was deployed on.

import { createHash } from 'node:crypto';
import fs from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';

import { deployContract, findDeployedContract, submitCallTx } from '@midnight-ntwrk/midnight-js-contracts';
import { CompiledContract } from '@midnight-ntwrk/compact-js';

import { config, emit, eras, flag, intFlag, log, messagesOf, runCommands, txTimeoutSecs, UsageError, withTimeout } from './lib/config.mjs';
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

// Each ledger serialises a contract's state under its own tag and decodes only its own.
const decodeContractState = (raw) => {
  for (const ledger of Object.values(eras.ledgers())) {
    try {
      return ledger.ContractState.deserialize(raw);
    } catch {
      // the other ledger's bytes
    }
  }
  return undefined;
};
const canonical = (v) => JSON.stringify(v, (_k, x) => {
  if (x instanceof Uint8Array) return Buffer.from(x).toString('hex');
  if (typeof x === 'bigint') return x.toString();
  return x instanceof Map ? [...x.entries()] : x;
});

// --height reads the state as of that block; without it, the latest.
const atHeight = () => {
  const blockHeight = intFlag('height', undefined, 0);
  return blockHeight === undefined ? undefined : { type: 'blockHeight', blockHeight };
};

const repeat = async (times, fn) => {
  const results = [];
  for (let i = 0; i < times; i += 1) results.push(await fn(i));
  return results;
};

runCommands({
  async head() {
    const version = await publicDataProvider().queryLatestProtocolVersion();
    const forks = resolvedForks();
    emit({ headProtocolVersion: version, forks, era: eras.of(version, forks) });
  },

  // The provider picks the decoder from the era the indexer labels the bytes with, so this
  // fails when the indexer serves bytes of the other era.
  async 'decode-state'() {
    if (!required('address')) return 2;
    const address = flag('address');
    const at = atHeight();
    const provider = publicDataProvider();
    let base = { address, height: at?.blockHeight ?? null };
    try {
      const read = await provider.queryContractState(address, at);
      if (!read) {
        emit({ ...base, decoded: false });
        return 1;
      }
      const raw = read.contractState.serialize();
      const tag = new TextDecoder().decode(raw.slice(0, 40)).match(/midnight:contract-state\[v\d+\]/)?.[0] ?? null;
      base = { ...base, labelledEra: read.contractState.era, protocolVersion: read.protocolVersion, tag };
      const ledgers = eras.ledgers();
      const decoded = Boolean(read.contractState.deserialize(eras.each(ledgers[eras.old].ContractState, ledgers[eras.new].ContractState)));
      emit({ ...base, decoded });
      return decoded ? 0 : 1;
    } catch (err) {
      log(`FAIL: ${err?.name ?? 'Error'}: ${String(err?.message ?? err).slice(0, 220)}`);
      emit({ ...base, decoded: false, error: err?.name ?? 'Error' });
      return 1;
    }
  },

  // Funds a contract holds; the fork must carry them over unchanged.
  async balances() {
    if (!required('address')) return 2;
    const address = flag('address');
    const at = atHeight();
    const balances = (await publicDataProvider().queryUnshieldedBalances(address, at)) ?? [];
    emit({
      address,
      height: at?.blockHeight ?? null,
      balances: Object.fromEntries(
        balances.filter((b) => BigInt(b.balance) !== 0n).map((b) => [String(b.tokenType).replace(/^0x/, ''), String(b.balance)]),
      ),
    });
  },

  // A transaction found by id and decoded with the ledger of the era it was made in.
  async 'tx-data'() {
    if (!required('tx-id')) return 2;
    const txId = flag('tx-id');
    const data = await withTimeout(publicDataProvider().watchForTxData(txId), 60, 'watchForTxData');
    emit({
      txId,
      version: data.era,
      status: data.status,
      blockHeight: data.blockHeight,
      protocolVersion: data.protocolVersion,
      decoded: Boolean(data.tx),
    });
  },

  'state-data'() {
    if (!required('hex-file')) return 2;
    const raw = Buffer.from(fs.readFileSync(flag('hex-file'), 'utf8').trim().replace(/^0x/, ''), 'hex');
    const tag = raw.subarray(0, 48).toString('latin1').match(/contract-state\[v\d+\]/)?.[0];
    const state = decodeContractState(raw);
    if (!state) throw new Error(`neither ledger decodes '${tag ?? 'no tag'}'`);
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
    const callArgs = JSON.parse(flag('args', '[]'));
    const expectError = flag('expect-error');
    return withProviders(t, async (providers) => {
      const found = await findDeployedContract(providers, {
        compiledContract: t.compiledContract,
        contractAddress: address,
        privateStateId: t.name,
        initialPrivateState: t.initialPrivateState,
      });
      if (!found.callTx[circuit]) throw new UsageError(`${t.name} has no circuit '${circuit}'`);
      // The circuit runs locally first: a failing assertion must surface before anything is proved or sent.
      if (expectError !== undefined) {
        try {
          const { public: pub = {} } = await inTime(found.callTx[circuit](...callArgs), `${circuit}`);
          log(`FAIL: ${circuit} landed at block ${pub.blockHeight}, expected it to fail with '${expectError}'`);
          emit({ refused: false, name: t.name, address, circuit, txId: pub.txId });
          return 1;
        } catch (err) {
          const message = messagesOf(err).join(' | ');
          const matched = message.includes(expectError);
          if (!matched) log(`FAIL: ${circuit} failed, but not with '${expectError}': ${message.slice(0, 220)}`);
          emit({ refused: true, matched, name: t.name, address, circuit, error: err?.name ?? 'Error', message: message.slice(0, 500) });
          return matched ? 0 : 1;
        }
      }
      const results = await repeat(times, async () => {
        const { public: pub = {} } = await inTime(found.callTx[circuit](...callArgs), `${circuit}`);
        return { txId: pub.txId, blockHeight: pub.blockHeight, status: pub.status };
      });
      emit({ called: true, name: t.name, address, circuit, times, results });
    });
  },

  // A pre-fork contract called after the fork with its old-ledger artefacts, several times:
  // one that could be called only once would be a regression.
  async 'retained-call'() {
    if (!required('contract-dir', 'address')) return 2;
    const times = intFlag('times', 3, 1);
    const t = target(true);
    const address = flag('address');
    const circuit = flag('circuit', 'increment');
    const psId = `${t.name}-retained-${address.slice(0, 8)}`;
    return withProviders(t, async (providers) => {
      const privateStates = providers.privateStateProvider.forContract(address);
      if ((await privateStates.get(psId)) == null) await privateStates.set(psId, t.initialPrivateState ?? {});
      const results = await repeat(times, async (i) => {
        const { public: pub = {} } = await inTime(submitCallTx(providers, {
          compiledContract: t.compiledContract,
          contractAddress: address,
          circuitId: circuit,
          privateStateId: psId,
        }), `${circuit}`);
        log(`  ${circuit} ${i + 1}/${times}: ${pub.status} at block ${pub.blockHeight}, era ${pub.era}`);
        return { txId: pub.txId, blockHeight: pub.blockHeight, status: pub.status, version: pub.era };
      });
      const privateState = await privateStates.get(psId);
      emit({ retainedCall: true, name: t.name, address, circuit, times, results, privateState });
    });
  },
});
