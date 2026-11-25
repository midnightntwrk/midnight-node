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

// Substrate reads the toolkit does not make, and a Root call through governance.
//
//   chain_check.mjs safe-mode
//   chain_check.mjs events --from <n> --to <m> [--sections safeMode,multiBlockMigrations]
//   chain_check.mjs storage --pallet <name> [--item <name>] [--args json-array]
//   chain_check.mjs governance-root-call --call section.method [--args json-array]
//        --council-uris a,b --tc-uris c,d --executor-uri e [--expect-event section.Method]
//
// The node is MN_NODE_WS, or --ws.

import { ApiPromise, WsProvider, Keyring } from '@polkadot/api';
import { blake2AsHex } from '@polkadot/util-crypto';
import { BN } from '@polkadot/util';
import { emit, flag, intFlag, log, runCommands, txTimeoutSecs, UsageError, withTimeout } from './lib/cli.mjs';

const wsUrl = flag('ws', process.env['MN_NODE_WS']);
const list = (s) => (s || '').split(',').map((x) => x.trim()).filter(Boolean);

const withApi = async (fn) => {
  // WsProvider falls back to ws://127.0.0.1:9944, which would test whatever runs there.
  if (!wsUrl) throw new Error('missing env MN_NODE_WS (or --ws)');
  const api = await withTimeout(ApiPromise.create({ provider: new WsProvider(wsUrl), noInitWarn: true }), 60, `connecting to ${wsUrl}`);
  try {
    return await fn(api);
  } finally {
    await api.disconnect();
  }
};

const headNumber = async (api) => (await api.rpc.chain.getHeader()).number.toNumber();

// Events of one block. polkadot-js decodes a block with its parent's runtime, which fails
// for the first block of a new runtime; that block is decoded with the next block's metadata.
const blockEvents = async (api, n) => {
  const hash = await api.rpc.chain.getBlockHash(n);
  const entry = { height: n, hash: hash.toHex() };
  try {
    const at = await api.at(hash);
    entry.specVersion = (await at.query.system.lastRuntimeUpgrade()).unwrapOr(null)?.specVersion?.toNumber() ?? null;
    entry.records = await at.query.system.events();
  } catch (err) {
    try {
      const next = await api.at(await api.rpc.chain.getBlockHash(n + 1));
      const raw = await api.rpc.state.getStorage(api.query.system.events.key(), hash);
      entry.records = next.registry.createType('Vec<FrameSystemEventRecord>', raw.toU8a(true));
      entry.decodedWithNextMetadata = true;
    } catch {
      entry.decodeError = String(err?.message ?? err).slice(0, 160);
    }
  }
  return entry;
};

const governance = (api, keyring) => {
  const closeWeight = () => api.createType('WeightV2', { refTime: new BN(10_000_000_000), proofSize: new BN(65_536) });

  // Settles on inclusion, on a dispatch error, on a terminal pool status and on a timeout:
  // a governance step that never settles would leave the chain in safe mode.
  const signAndWait = (extrinsic, signer, label) => new Promise((resolve, reject) => {
    const secs = txTimeoutSecs();
    let unsub;
    let settled = false;
    const settle = (fn, value) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      unsub?.();
      fn(value);
    };
    const timer = setTimeout(() => settle(reject, new Error(`${label}: not in a block after ${secs} s (MN_TX_TIMEOUT_SECS; it may still land)`)), secs * 1000);
    extrinsic.signAndSend(signer, { nonce: -1 }, (result) => {
      const { dispatchError, status } = result;
      if (dispatchError) {
        let msg = dispatchError.toString();
        if (dispatchError.isModule) {
          const meta = dispatchError.registry.findMetaError(dispatchError.asModule);
          msg = `${meta.section}.${meta.name}: ${meta.docs.join(' ')}`;
        }
        settle(reject, new Error(`${label} failed: ${msg}`));
      } else if (status.isInBlock) {
        log(`${label} in block ${status.asInBlock.toHex().slice(0, 18)}…`);
        settle(resolve, result);
      } else if (status.isDropped || status.isUsurped || status.isInvalid || status.isFinalityTimeout) {
        settle(reject, new Error(`${label}: ${status.type}`));
      }
    }).then((u) => { unsub = u; if (settled) u(); }, (err) => settle(reject, err));
  });

  // Propose, vote with every signer and close: a two-thirds motion of one collective.
  const passMotion = async (collective, uris, call) => {
    const members = (await api.query[collective].members()).toJSON();
    if (!members.length) throw new Error(`${collective} has no members`);
    const threshold = Math.ceil((members.length * 2) / 3);
    const signers = uris.map((u, i) => keyring.addFromUri(u, { name: `${collective} ${i + 1}` }));
    if (signers.length < threshold) throw new Error(`${collective}: ${signers.length} signers, need ${threshold}`);
    const lengthBound = call.encodedLength;
    const proposed = await signAndWait(api.tx[collective].propose(threshold, call, lengthBound), signers[0], `${collective}.propose`);
    const ev = proposed.events.find(({ event }) => event.section.toLowerCase() === collective.toLowerCase() && event.method === 'Proposed');
    if (!ev) throw new Error(`${collective}: no Proposed event`);
    const index = ev.event.data[1].toPrimitive();
    const hash = ev.event.data[2].toHex();
    for (const s of signers) await signAndWait(api.tx[collective].vote(hash, index, true), s, `${collective}.vote`);
    await signAndWait(api.tx[collective].close(hash, index, closeWeight(), lengthBound), signers[0], `${collective}.close`);
  };

  return { closeWeight, signAndWait, passMotion };
};

runCommands({
  async 'safe-mode'() {
    return withApi(async (api) => {
      const present = Boolean(api.query.safeMode);
      const head = await headNumber(api);
      let enteredUntil = null;
      if (present) {
        const v = await api.query.safeMode.enteredUntil();
        enteredUntil = v.isSome ? v.unwrap().toNumber() : null;
      }
      const pallet = api.runtimeMetadata.asLatest.pallets.find((p) => p.name.toString() === 'SafeMode');
      emit({
        ws: wsUrl,
        specVersion: api.runtimeVersion.specVersion.toNumber(),
        head,
        palletPresent: present,
        palletIndex: present && pallet ? pallet.index.toNumber() : null,
        isEntered: enteredUntil !== null,
        enteredUntil,
        blocksRemaining: enteredUntil === null ? null : enteredUntil - head,
        consts: Object.fromEntries(Object.entries(api.consts.safeMode ?? {}).map(([k, v]) => [k, v.toString()])),
      });
    });
  },

  // --sections filters on the pallet name, case-insensitively.
  async events() {
    const from = intFlag('from', undefined, 0);
    const to = intFlag('to', undefined, 0);
    const sections = list(flag('sections')).map((s) => s.toLowerCase());
    if (from === undefined || to === undefined || to < from) throw new UsageError('events --from <n> --to <m> [--sections a,b]');
    return withApi(async (api) => {
      const blocks = [];
      const counts = {};
      for (let n = from; n <= to; n += 1) {
        const { records = [], ...entry } = await blockEvents(api, n);
        entry.events = [];
        for (const { event } of records) {
          if (sections.length && !sections.includes(event.section.toLowerCase())) continue;
          const key = `${event.section}.${event.method}`;
          counts[key] = (counts[key] ?? 0) + 1;
          entry.events.push({ section: event.section, method: event.method, data: event.data.toJSON() });
        }
        blocks.push(entry);
        const note = entry.decodeError ? ' (decode error)' : entry.decodedWithNextMetadata ? " (decoded with the next block's metadata)" : '';
        const names = entry.events.map((e) => `${e.section}.${e.method}`).join(', ');
        log(`#${n}${note}${names ? ': ' + names : ''}`);
      }
      // A block whose events could not be read must not pass for a block without events.
      const undecoded = blocks.filter((b) => b.decodeError).map((b) => b.height);
      emit({ from, to, counts, decodeErrors: { count: undecoded.length, blocks: undecoded }, blocks });
    });
  },

  // Without --item: is the pallet there, and which items it has.
  async storage() {
    const pallet = flag('pallet');
    const item = flag('item');
    if (!pallet) {
      log('usage: storage --pallet <camelCaseName> [--item <name>] [--args json-array]');
      return 2;
    }
    return withApi(async (api) => {
      const q = api.query[pallet];
      if (!q) { emit({ pallet, present: false, items: [] }); return 1; }
      if (!item) { emit({ pallet, present: true, items: Object.keys(q) }); return 0; }
      if (!q[item]) { emit({ pallet, present: true, item, error: 'no such storage item', items: Object.keys(q) }); return 1; }
      const v = await q[item](...JSON.parse(flag('args', '[]')));
      emit({ pallet, item, value: v.toJSON(), human: v.toHuman() });
      return 0;
    });
  },

  // Both collectives pass a FederatedAuthority.motion_approve of the call, then the
  // executor closes the motion, which dispatches the call as Root.
  async 'governance-root-call'() {
    const target = flag('call');
    const councilUris = list(flag('council-uris'));
    const tcUris = list(flag('tc-uris'));
    const executorUri = flag('executor-uri');
    if (!target?.includes('.') || !councilUris.length || !tcUris.length || !executorUri) {
      log('usage: governance-root-call --call section.method [--args json-array] --council-uris a,b --tc-uris c,d --executor-uri e [--expect-event section.Method]');
      return 2;
    }
    const [section, method] = target.split('.');
    const callArgs = JSON.parse(flag('args', '[]'));
    const expectEvent = flag('expect-event');
    return withApi(async (api) => {
      if (!api.tx[section]?.[method]) throw new Error(`no call ${section}.${method} in this runtime`);
      const keyring = new Keyring({ type: 'sr25519' });
      const { closeWeight, signAndWait, passMotion } = governance(api, keyring);
      const inner = api.tx[section][method](...callArgs);
      const approve = api.tx.federatedAuthority.motionApprove(inner.method);
      const motionHash = blake2AsHex(inner.method.toU8a());
      log(`root call ${target}(${JSON.stringify(callArgs)}) -> motion ${motionHash.slice(0, 18)}…`);
      await passMotion('council', councilUris, approve.method);
      await passMotion('technicalCommittee', tcUris, approve.method);
      const motionClose = api.tx.federatedAuthority.motionClose;
      const closeTx = motionClose.meta.args.length === 1 ? motionClose(motionHash) : motionClose(motionHash, closeWeight());
      const result = await signAndWait(closeTx, keyring.addFromUri(executorUri, { name: 'executor' }), 'federatedAuthority.motionClose');
      const events = result.events.map(({ event }) => ({ section: event.section, method: event.method, data: event.data.toJSON() }));
      const names = events.map((e) => `${e.section}.${e.method}`);
      const seen = !expectEvent || names.some((n) => n.toLowerCase() === expectEvent.toLowerCase());
      emit({
        call: target, args: callArgs, motionHash, executedAtOrBefore: await headNumber(api),
        events, expectedEvent: expectEvent ?? null, expectedEventSeen: seen,
      });
      if (!seen) {
        log(`FAIL: expected event ${expectEvent} not among ${names.join(', ')}`);
        return 1;
      }
      return 0;
    });
  },
});
