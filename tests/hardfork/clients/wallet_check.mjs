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

// Wallet SDK checks across the fork.
//
//   wallet_check.mjs probe                                   fork schedule against the chain head
//   wallet_check.mjs sync                                    sync and print balances and history
//   wallet_check.mjs send [--kind shielded|unshielded] [--amount n] [--token t] [--to <address>]
//   wallet_check.mjs dust-register                           register NIGHT for DUST generation
//   wallet_check.mjs restore [--expect-json <sync output>]   fresh wallet, balances compared
//   wallet_check.mjs save --out <file>                       sync, then write the serialized wallets
//   wallet_check.mjs resume --from <file> [--expect-json <sync output>]
//                                                            restore that snapshot and sync on from it
//   wallet_check.mjs soak                                    one wallet sending across the fork (LOAD_*)
//   wallet_check.mjs parse-address [--unshielded a] [--shielded a] [--dust a]
//                                                            decode addresses as an application does

import { readFile, writeFile } from 'node:fs/promises';
import { setTimeout as sleep } from 'node:timers/promises';
import { DustAddress, MidnightBech32m, ShieldedAddress, UnshieldedAddress } from '@midnightntwrk/wallet-sdk';
import { config, emit, eras, flag, intEnv, intFlag, log, nodeRefusal, runCommands, withTimeout } from './lib/config.mjs';
import { historySummary, resolvedForks, serializeWallets, settledState, submit, summarise, ttl, withWallet } from './lib/wallet.mjs';

const NIGHT = '0'.repeat(64);
// Only NIGHT generates DUST: the SDK refuses a registration that includes any other token.
const nightCoins = (state) => (state?.unshielded?.availableCoins ?? []).filter((c) => c.utxo.type === NIGHT);

const indexerHead = async () => {
  const res = await fetch(config.indexerHttp, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ query: '{ block { height hash protocolVersion } }' }),
  });
  if (!res.ok) throw new Error(`indexer ${config.indexerHttp} answered HTTP ${res.status}`);
  const block = (await res.json())?.data?.block;
  if (!block) throw new Error('indexer returned no block');
  return block;
};

// Order-independent, so two wallets that found the same tokens in another order match.
const balancesJson = (wallet, kind) =>
  JSON.stringify(Object.entries(wallet?.[kind]?.balances ?? {}).map(([t, v]) => [t, String(v)]).sort());

const compareBalances = async (wallet, expectedFile, what) => {
  const want = JSON.parse(await readFile(expectedFile, 'utf8')).wallet;
  const mismatched = ['unshielded', 'shielded'].filter((kind) => {
    const [got, wanted] = [balancesJson(wallet, kind), balancesJson(want, kind)];
    if (got !== wanted) log(`FAIL: ${what} ${kind} balances ${got} != live wallet's ${wanted}`);
    return got !== wanted;
  });
  return mismatched.length > 0 ? 1 : 0;
};

// A bech32m address string, decoded the way an application takes one from a user.
const receiver = (kind, own) => {
  const to = flag('to');
  if (!to) return own;
  return MidnightBech32m.parse(to).decode(kind === 'shielded' ? ShieldedAddress : UnshieldedAddress, config.networkId);
};

const errorOf = (err) => ({ name: err?.name ?? 'Error', message: String(err?.message ?? err).slice(0, 300), ...nodeRefusal(err) });

// What became of a submitted transaction, from the wallet's own history.
const verdictOf = async (facade, txId, secs) => {
  const until = Date.now() + secs * 1000;
  while (Date.now() < until) {
    const entry = (await facade.getAllFromTxHistory()).find((e) => e.hash === txId || e.identifiers?.includes(txId));
    const lifecycle = entry?.lifecycle;
    if (lifecycle?.status === 'finalized') {
      return { outcome: entry.status === 'FAILURE' ? 'failed' : 'landed', blockHeight: lifecycle.finalizedBlock.height, protocolVersion: entry.protocolVersion };
    }
    if (lifecycle?.status === 'rejected') return { outcome: 'rejected', reason: lifecycle.reason };
    await sleep(3000);
  }
  return { outcome: 'timeout' };
};

runCommands({
  // No wallet needed: what a dApp does with an address a user pastes.
  'parse-address'() {
    const kinds = { unshielded: UnshieldedAddress, shielded: ShieldedAddress, dust: DustAddress };
    const results = Object.entries(kinds)
      .filter(([kind]) => flag(kind))
      .map(([kind, cls]) => {
        const address = flag(kind);
        try {
          MidnightBech32m.parse(address).decode(cls, config.networkId);
          return { kind, length: address.length, parsed: true };
        } catch (err) {
          return { kind, length: address.length, parsed: false, error: String(err?.message ?? err).slice(0, 200) };
        }
      });
    emit({ results });
    return results.every((r) => r.parsed) ? 0 : 1;
  },

  async probe() {
    const forks = resolvedForks();
    const head = await indexerHead();
    emit({
      forks,
      chainHead: head,
      eraForHead: eras.of(head.protocolVersion, forks),
      provers: eras.each(config.proofServerOld, config.proofServerNew),
    });
  },

  async sync() {
    const head = await indexerHead();
    return withWallet(async ({ facade, state }) =>
      emit({ chainHead: head, wallet: summarise(state), history: await historySummary(facade, resolvedForks()) }),
    );
  },

  // To the wallet's own address unless --to names another.
  async send() {
    const kind = flag('kind', 'unshielded');
    const amount = BigInt(intFlag('amount', 1, 1));
    const tokenType = flag('token', NIGHT);
    return withWallet(async ({ facade, keystore, state }) => {
      const receiverAddress = receiver(kind, kind === 'shielded' ? state.shielded.address : state.unshielded.address);
      log(`building a ${kind} transfer of ${amount} ${tokenType === NIGHT ? 'NIGHT' : tokenType}${flag('to') ? ` to ${flag('to')}` : ''}`);
      const recipe = await facade.transferTransaction([{ type: kind, outputs: [{ type: tokenType, receiverAddress, amount }] }], { ttl: ttl() });
      const signed = await facade.signRecipe(recipe, keystore.signDataAsync);
      log(`proving at protocol version ${recipe.protocolVersion}`);
      const txId = await submit(facade, await facade.finalizeRecipe(signed));
      emit({ submitted: true, txId, kind, amount, tokenType, to: flag('to') ?? 'self', provedAtProtocolVersion: recipe.protocolVersion });
    });
  },

  // After a fork that resets DUST, a wallet pays for re-registration from the DUST its
  // NIGHT generated since.
  async 'dust-register'() {
    return withWallet(async ({ facade, keystore }) => {
      const state = await settledState(facade);
      log(`wallets settled on protocol version ${state.protocol.version}`);
      const utxos = nightCoins(state);
      if (utxos.length === 0) {
        log('FAIL: no NIGHT UTxOs to register');
        return 1;
      }
      const estimate = await facade.estimateRegistration(utxos);
      log(`registration fee estimate: ${estimate.fee}`);
      const recipe = await facade.registerNightUtxosForDustGeneration(utxos, keystore.getPublicKey(), keystore.signDataAsync);
      const txId = await submit(facade, await facade.finalizeRecipe(recipe));
      emit({ registered: true, txId, utxos: utxos.length, feeEstimate: estimate.fee });
    });
  },

  // A wallet new to this chain replays it from genesis across the fork and must reach the
  // live wallet's balances. DUST keeps generating, so it is not compared.
  async restore() {
    const expected = flag('expect-json');
    const { wallet, syncMs } = await withWallet(({ state, syncMs }) => ({ wallet: summarise(state), syncMs }));
    emit({ restoredIn: syncMs, peakRssMb: Math.round(process.resourceUsage().maxRSS / 1024), wallet });
    return expected ? compareBalances(wallet, expected, 'restored') : 0;
  },

  // An application that persisted its wallet before the fork.
  async save() {
    const out = flag('out');
    if (!out) {
      log('missing --out');
      return 2;
    }
    const head = await indexerHead();
    return withWallet(async ({ facade, state }) => {
      await writeFile(out, JSON.stringify(await serializeWallets(facade)), { mode: 0o600 });
      emit({ saved: out, chainHead: head, protocolVersion: state.activeProtocolVersion, wallet: summarise(state) });
    });
  },

  // ...and opens it again after the fork: the snapshot was written below the boundary.
  async resume() {
    const from = flag('from');
    if (!from) {
      log('missing --from');
      return 2;
    }
    const snapshot = JSON.parse(await readFile(from, 'utf8'));
    const expected = flag('expect-json');
    const { wallet, syncMs, version } = await withWallet(
      ({ state, syncMs }) => ({ wallet: summarise(state), syncMs, version: state.activeProtocolVersion }),
      { snapshot },
    );
    emit({ resumedIn: syncMs, protocolVersion: version, wallet });
    return expected ? compareBalances(wallet, expected, 'resumed') : 0;
  },

  // One wallet process across the fork: a NIGHT transfer to itself every LOAD_INTERVAL_SECS,
  // each followed to its verdict. After the fork it re-registers for DUST once its sends fail,
  // and stops LOAD_AFTER_FORK_SECS after it first acted on the new ledger.
  async soak() {
    const intervalMs = intEnv('LOAD_INTERVAL_SECS', 5, 0) * 1000;
    const afterForkMs = intEnv('LOAD_AFTER_FORK_SECS', 300, 1) * 1000;
    const deadline = Date.now() + intEnv('LOAD_MAX_SECS', 3600, 60) * 1000;
    const registerAfterMs = intEnv('LOAD_REGISTER_AFTER_SECS', 60, 0) * 1000;
    const txSecs = intEnv('LOAD_TX_TIMEOUT_SECS', 240, 10);
    const forks = resolvedForks();
    const forkVersion = Number(forks[eras.new]);
    const now = () => new Date().toISOString();
    return withWallet(async ({ facade, keystore, state }) => {
      const phases = [];
      const orphaned = new Map();
      let latest = state;
      let forkSeenAt;
      const observe = (st) => {
        latest = st;
        const p = st.protocol;
        const key = p._tag === 'Settled' ? `Settled ${p.version}` : `Crossing ${p.from} -> ${p.to}, behind: ${p.behind.join(',')}`;
        if (phases.at(-1)?.key !== key) {
          phases.push({ key, tag: p._tag, at: now(), versions: { ...st.protocolVersion } });
          log(`protocol: ${key}`);
        }
        if (forkSeenAt === undefined && Number(st.activeProtocolVersion) >= forkVersion) forkSeenAt = Date.now();
        for (const entry of st.lifecycle) {
          if (entry.status._tag === 'Orphaned') orphaned.set(entry.identifiers[0], { authoredFor: entry.status.authoredFor, chainNow: entry.status.chainNow });
        }
      };
      observe(state);
      const subscription = facade.state().subscribe(observe);
      const attempts = [];
      const registrations = [];
      try {
        while (Date.now() < deadline && !(forkSeenAt !== undefined && Date.now() - forkSeenAt >= afterForkMs)) {
          const a = { i: attempts.length + 1, startedAt: now(), versionAtStart: Number(latest?.activeProtocolVersion), phaseAtStart: latest?.protocol?._tag };
          try {
            const recipe = await facade.transferTransaction(
              [{ type: 'unshielded', outputs: [{ type: NIGHT, receiverAddress: latest.unshielded.address, amount: 1n }] }],
              { ttl: ttl() },
            );
            a.builtFor = Number(recipe.protocolVersion);
            const tx = await facade.finalizeRecipe(await facade.signRecipe(recipe, keystore.signDataAsync));
            a.txId = await withTimeout(facade.submitTransaction(tx), txSecs, 'submitTransaction');
            a.submittedAt = now();
            Object.assign(a, await verdictOf(facade, a.txId, txSecs));
          } catch (err) {
            a.outcome = /no result after/.test(String(err?.message)) ? 'timeout' : 'refused';
            a.error = errorOf(err);
          }
          a.endedAt = now();
          a.versionAtEnd = Number(latest?.activeProtocolVersion);
          attempts.push(a);
          log(`#${a.i} built for ${a.builtFor ?? '-'}: ${a.outcome}${a.blockHeight ? ` at #${a.blockHeight}` : ''}${a.error ? ` (${a.error.name}: ${a.error.message.slice(0, 120)})` : ''}`);
          const landedSinceFork = attempts.some((x) => x.outcome === 'landed' && x.builtFor >= forkVersion);
          const lastTry = registrations.at(-1)?.at ?? 0;
          if (forkSeenAt !== undefined && !landedSinceFork && !registrations.some((r) => r.ok)
              && Date.now() - forkSeenAt >= registerAfterMs && registrations.length < 3 && Date.now() - lastTry >= 60_000) {
            const r = { at: Date.now(), startedAt: now() };
            try {
              const utxos = nightCoins(latest);
              const recipe = await facade.registerNightUtxosForDustGeneration(utxos, keystore.getPublicKey(), keystore.signDataAsync);
              r.txId = await submit(facade, await facade.finalizeRecipe(recipe));
              Object.assign(r, await verdictOf(facade, r.txId, txSecs));
              r.ok = r.outcome === 'landed';
            } catch (err) {
              r.ok = false;
              r.error = errorOf(err);
            }
            registrations.push(r);
            log(`DUST re-registration: ${r.ok ? 'landed' : `failed: ${r.outcome ?? ''} ${r.error?.message?.slice(0, 120) ?? ''}`}`);
          }
          await sleep(intervalMs);
        }
      } finally {
        subscription.unsubscribe();
      }
      const final = await withTimeout(facade.waitForSyncedState(), 120, 'final sync').catch(() => latest);
      emit({
        forks,
        forkSeenAt: forkSeenAt === undefined ? null : new Date(forkSeenAt).toISOString(),
        phases,
        orphaned: [...orphaned.values()],
        attempts,
        registrations,
        finalPending: final?.lifecycle?.map((entry) => entry.status._tag) ?? [],
        finalVersion: Number(final?.activeProtocolVersion),
        history: await historySummary(facade, forks),
      });
    });
  },
});
